//! Admin CLI subcommands for operational tasks.
//!
//! These commands are intended to be wired into the `forged` binary's CLI
//! (e.g. `forged admin migrate-status` or `forged admin build-cleanup --older-than-days 30`).

use crate::repositories::BuildJobRepository;
use crate::Settings;
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::Database;
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
}

/// Execute an admin command against the database configured in `settings`.
pub async fn run_admin_command(settings: &Settings, command: AdminCommand) -> Result<()> {
    match command {
        AdminCommand::MigrateStatus => run_migrate_status(settings).await,
        AdminCommand::BuildCleanup { older_than_days } => {
            run_build_cleanup(settings, older_than_days).await
        }
    }
}

/// Run pending migrations, then print the current status.
async fn run_migrate_status(settings: &Settings) -> Result<()> {
    use sea_orm_migration::MigratorTrait;

    let db = Database::connect(&settings.postgres.url)
        .await
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "Failed to connect to PostgreSQL at '{}'.\n\
                 Verify the connection string in your configuration (postgres.url) \
                 and ensure the database server is running.",
                settings.postgres.url
            )
        })?;

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
    let db = Database::connect(&settings.postgres.url)
        .await
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "Failed to connect to PostgreSQL at '{}'.\n\
                 Verify the connection string in your configuration (postgres.url) \
                 and ensure the database server is running.",
                settings.postgres.url
            )
        })?;

    let cutoff = chrono::Utc::now().fixed_offset()
        - chrono::Duration::days(i64::from(older_than_days));

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
