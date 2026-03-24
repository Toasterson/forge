//! Admin CLI subcommands for operational tasks.
//!
//! These commands are intended to be wired into the `forged` binary's CLI
//! (e.g. `forged admin migrate-status` or `forged admin build-cleanup --older-than-days 30`).

use crate::entities::server_member;
use crate::repositories::BuildJobRepository;
use crate::services::{server_role_defaults, ServerRole};
use crate::Settings;
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use std::sync::Arc;

/// Admin subcommands available via the CLI.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum AdminCommand {
    /// Run pending migrations and print current migration status.
    MigrateStatus,

    /// Delete completed build jobs older than N days.
    BuildCleanup {
        /// Delete build jobs older than this many days.
        #[arg(long)]
        older_than_days: u32,
    },

    /// Add an actor as a server member with a role (admin or gate_creator).
    AddServerMember {
        /// The actor ID to add.
        #[arg(long)]
        actor_id: String,
        /// The server role: "admin" or "gate_creator".
        #[arg(long)]
        role: String,
    },

    /// Remove an actor from the server members.
    RemoveServerMember {
        /// The actor ID to remove.
        #[arg(long)]
        actor_id: String,
    },

    /// List all server members with their roles and permissions.
    ListServerMembers,
}

/// Execute an admin command against the database configured in `settings`.
pub async fn run_admin_command(settings: &Settings, command: AdminCommand) -> Result<()> {
    match command {
        AdminCommand::MigrateStatus => run_migrate_status(settings).await,
        AdminCommand::BuildCleanup { older_than_days } => {
            run_build_cleanup(settings, older_than_days).await
        }
        AdminCommand::AddServerMember { actor_id, role } => {
            run_add_server_member(settings, &actor_id, &role).await
        }
        AdminCommand::RemoveServerMember { actor_id } => {
            run_remove_server_member(settings, &actor_id).await
        }
        AdminCommand::ListServerMembers => run_list_server_members(settings).await,
    }
}

/// Connect to the database using the configured URL.
async fn connect_db(settings: &Settings) -> Result<DatabaseConnection> {
    Database::connect(&settings.postgres.url)
        .await
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "Failed to connect to PostgreSQL at '{}'.\n\
                 Verify the connection string in your configuration (postgres.url) \
                 and ensure the database server is running.",
                settings.postgres.url
            )
        })
}

/// Run pending migrations, then print the current status.
async fn run_migrate_status(settings: &Settings) -> Result<()> {
    use sea_orm_migration::MigratorTrait;

    let db = connect_db(settings).await?;

    println!("Running pending migrations...");
    migration::Migrator::up(&db, None)
        .await
        .into_diagnostic()
        .wrap_err(
            "Migration failed.\n\
             Check the migration files and database state for conflicts.",
        )?;

    println!("Checking migration status...");
    let status = migration::Migrator::get_pending_migrations(&db)
        .await
        .into_diagnostic()
        .wrap_err("Failed to query migration status")?;

    if status.is_empty() {
        println!("All migrations are up to date.");
    } else {
        println!("{} pending migration(s):", status.len());
        for m in &status {
            println!("  - {}", m.name());
        }
    }

    Ok(())
}

/// Delete terminal build jobs older than `older_than_days` days.
async fn run_build_cleanup(settings: &Settings, older_than_days: u32) -> Result<()> {
    let db = connect_db(settings).await?;

    let cutoff =
        chrono::Utc::now().fixed_offset() - chrono::Duration::days(i64::from(older_than_days));

    let repo = Arc::new(BuildJobRepository::new(db));
    let deleted = repo
        .delete_older_than(cutoff)
        .await
        .wrap_err("Failed to clean up old build jobs")?;

    println!(
        "Deleted {} build job(s) older than {} day(s) (cutoff: {}).",
        deleted, older_than_days, cutoff
    );

    Ok(())
}

/// Add or update a server member with the given role.
async fn run_add_server_member(settings: &Settings, actor_id: &str, role_str: &str) -> Result<()> {
    let server_role = ServerRole::from_str(role_str).ok_or_else(|| {
        miette::miette!(
            "Unknown server role: '{}'\n\
             Valid roles are: admin, gate_creator",
            role_str
        )
    })?;

    let permissions = server_role_defaults(&server_role);
    let role_json = serde_json::json!([role_str]);
    let perm_json = serde_json::json!(permissions.iter().map(|p| p.as_str()).collect::<Vec<_>>());

    let db = connect_db(settings).await?;

    // Check if the member already exists
    let existing = server_member::Entity::find()
        .filter(server_member::Column::ActorId.eq(actor_id))
        .one(&db)
        .await
        .into_diagnostic()
        .wrap_err("Failed to query server_member table")?;

    if let Some(existing) = existing {
        // Update the existing record
        let mut active: server_member::ActiveModel = existing.into();
        active.roles = Set(role_json);
        active.permissions = Set(perm_json);
        active.updated_at = Set(chrono::Utc::now().fixed_offset());
        active
            .update(&db)
            .await
            .into_diagnostic()
            .wrap_err("Failed to update server member")?;
        println!(
            "Updated server member '{}' with role '{}'.",
            actor_id, role_str
        );
    } else {
        // Insert a new record
        let now = chrono::Utc::now().fixed_offset();
        let active = server_member::ActiveModel {
            actor_id: Set(actor_id.to_string()),
            roles: Set(role_json),
            permissions: Set(perm_json),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        active
            .insert(&db)
            .await
            .into_diagnostic()
            .wrap_err("Failed to insert server member")?;
        println!(
            "Added server member '{}' with role '{}'.",
            actor_id, role_str
        );
    }

    Ok(())
}

/// Remove a server member by actor ID.
async fn run_remove_server_member(settings: &Settings, actor_id: &str) -> Result<()> {
    let db = connect_db(settings).await?;

    let result = server_member::Entity::delete_many()
        .filter(server_member::Column::ActorId.eq(actor_id))
        .exec(&db)
        .await
        .into_diagnostic()
        .wrap_err("Failed to delete server member")?;

    if result.rows_affected == 0 {
        println!("No server member found with actor_id '{}'.", actor_id);
    } else {
        println!("Removed server member '{}'.", actor_id);
    }

    Ok(())
}

/// List all server members.
async fn run_list_server_members(settings: &Settings) -> Result<()> {
    let db = connect_db(settings).await?;

    let members = server_member::Entity::find()
        .all(&db)
        .await
        .into_diagnostic()
        .wrap_err("Failed to list server members")?;

    if members.is_empty() {
        println!("No server members found.");
    } else {
        println!("{:<30} {:<20} PERMISSIONS", "ACTOR ID", "ROLES");
        println!("{}", "-".repeat(80));
        for m in &members {
            let roles = m
                .roles
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let perms = m
                .permissions
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            println!("{:<30} {:<20} {}", m.actor_id, roles, perms);
        }
    }

    Ok(())
}
