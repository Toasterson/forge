use crate::entities::{component, gate, gate_member, Component, Gate, GateMember};
use crate::storage::jj_repos::manager::JjRepoManager;
use crate::types::{GateId, RepoId};
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// Repository for gate management
#[derive(Clone)]
pub struct GateRepository {
    db: Arc<DatabaseConnection>,
    jj_manager: Arc<JjRepoManager>,
}

impl GateRepository {
    pub fn new(db: Arc<DatabaseConnection>, jj_manager: Arc<JjRepoManager>) -> Self {
        Self { db, jj_manager }
    }

    /// Create a new gate
    /// Also creates a Jujutsu repository for the gate
    pub async fn create_gate(
        &self,
        name: String,
        gate_kdl: String,
        owner_id: String,
    ) -> Result<gate::Model> {
        let gate_id = Uuid::new_v4().to_string();

        // 1. Create database record
        let model = gate::ActiveModel {
            id: Set(gate_id.clone()),
            name: Set(name.clone()),
            gate_kdl: Set(gate_kdl.clone()),
            owner_id: Set(owner_id.clone()),
            ..Default::default()
        };

        let gate = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert gate")?;

        // 2. Create Jujutsu repository
        let repo_id = RepoId::Gate(GateId(gate_id.clone()));
        let workspace = self
            .jj_manager
            .ensure_repo(&repo_id)
            .await
            .wrap_err("failed to create Jujutsu repository for gate")?;

        // 3. Write initial gate manifest to Jj repo
        let mut tx = workspace
            .start_transaction("Initialize gate")
            .map_err(|e| miette::miette!("failed to start jj transaction: {}", e))?;

        let manifest_path = workspace.workspace_root().join("gate.kdl");
        std::fs::write(&manifest_path, &gate_kdl)
            .into_diagnostic()
            .wrap_err("failed to write gate manifest")?;

        tx.commit("Initialize gate")
            .map_err(|e| miette::miette!("failed to commit to jj: {}", e))?;

        // 4. Reload workspace to pick up the commit
        self.jj_manager
            .reload_workspace(&repo_id)
            .await
            .wrap_err("failed to reload workspace after gate creation")?;

        tracing::info!(
            gate_id = %gate_id,
            name = %name,
            owner_id = %owner_id,
            "Created new gate"
        );

        Ok(gate)
    }

    /// Get a gate by ID
    pub async fn get_gate(&self, gate_id: &str) -> Result<Option<gate::Model>> {
        let gate = Gate::find_by_id(gate_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get gate")?;

        Ok(gate)
    }

    /// Update a gate's KDL
    /// Also commits the change to the Jujutsu repository
    pub async fn update_gate(&self, gate_id: &str, gate_kdl: String) -> Result<gate::Model> {
        let gate = self.get_gate(gate_id).await?.ok_or_else(|| {
            miette::miette!(
                "Gate not found: id={}. \n\
                     This gate may have been deleted or does not exist.",
                gate_id
            )
        })?;

        // 1. Update database record
        let mut active: gate::ActiveModel = gate.into();
        active.gate_kdl = Set(gate_kdl.clone());
        active.updated_at = Set(chrono::Utc::now().into());

        let updated = active
            .update(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to update gate")?;

        // 2. Update Jujutsu repository
        let repo_id = RepoId::Gate(GateId(gate_id.to_string()));
        let workspace = self
            .jj_manager
            .ensure_repo(&repo_id)
            .await
            .wrap_err("failed to get Jujutsu workspace for gate")?;

        let mut tx = workspace
            .start_transaction("Update gate")
            .map_err(|e| miette::miette!("failed to start jj transaction: {}", e))?;

        let manifest_path = workspace.workspace_root().join("gate.kdl");
        std::fs::write(&manifest_path, &gate_kdl)
            .into_diagnostic()
            .wrap_err("failed to write gate manifest")?;

        tx.commit("Update gate")
            .map_err(|e| miette::miette!("failed to commit to jj: {}", e))?;

        self.jj_manager
            .reload_workspace(&repo_id)
            .await
            .wrap_err("failed to reload workspace after gate update")?;

        tracing::info!(
            gate_id = %gate_id,
            "Updated gate"
        );

        Ok(updated)
    }

    /// List all gates
    pub async fn list_all(&self) -> Result<Vec<gate::Model>> {
        let gates = Gate::find()
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list gates")?;

        Ok(gates)
    }

    /// List gates owned by an actor
    pub async fn list_by_owner(&self, owner_id: &str) -> Result<Vec<gate::Model>> {
        let gates = Gate::find()
            .filter(gate::Column::OwnerId.eq(owner_id))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list gates by owner")?;

        Ok(gates)
    }

    /// Add a member to a gate
    pub async fn add_member(
        &self,
        gate_id: &str,
        actor_id: &str,
        roles: Vec<String>,
        permissions: Vec<String>,
    ) -> Result<gate_member::Model> {
        // Verify gate exists
        self.get_gate(gate_id).await?.ok_or_else(|| {
            miette::miette!(
                "Gate not found: id={}. Cannot add member to non-existent gate.",
                gate_id
            )
        })?;

        // Check if member already exists
        if let Some(existing) = self.get_member(gate_id, actor_id).await? {
            return Err(miette::miette!(
                "Actor {} is already a member of gate {}",
                actor_id,
                gate_id
            ));
        }

        let model = gate_member::ActiveModel {
            gate_id: Set(gate_id.to_string()),
            actor_id: Set(actor_id.to_string()),
            roles: Set(json!(roles)),
            permissions: Set(json!(permissions)),
            ..Default::default()
        };

        let member = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert gate member")?;

        tracing::info!(
            gate_id = %gate_id,
            actor_id = %actor_id,
            "Added gate member"
        );

        Ok(member)
    }

    /// Get a gate member
    pub async fn get_member(
        &self,
        gate_id: &str,
        actor_id: &str,
    ) -> Result<Option<gate_member::Model>> {
        let member = GateMember::find()
            .filter(gate_member::Column::GateId.eq(gate_id))
            .filter(gate_member::Column::ActorId.eq(actor_id))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get gate member")?;

        Ok(member)
    }

    /// List all members of a gate
    pub async fn list_members(&self, gate_id: &str) -> Result<Vec<gate_member::Model>> {
        let members = GateMember::find()
            .filter(gate_member::Column::GateId.eq(gate_id))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list gate members")?;

        Ok(members)
    }

    /// Update a gate member's roles and permissions
    pub async fn update_member(
        &self,
        gate_id: &str,
        actor_id: &str,
        roles: Vec<String>,
        permissions: Vec<String>,
    ) -> Result<gate_member::Model> {
        let member = self.get_member(gate_id, actor_id).await?.ok_or_else(|| {
            miette::miette!(
                "Gate member not found: gate_id={}, actor_id={}",
                gate_id,
                actor_id
            )
        })?;

        let mut active: gate_member::ActiveModel = member.into();
        active.roles = Set(json!(roles));
        active.permissions = Set(json!(permissions));
        active.updated_at = Set(chrono::Utc::now().into());

        let updated = active
            .update(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to update gate member")?;

        tracing::info!(
            gate_id = %gate_id,
            actor_id = %actor_id,
            "Updated gate member"
        );

        Ok(updated)
    }

    /// Remove a member from a gate
    pub async fn remove_member(&self, gate_id: &str, actor_id: &str) -> Result<()> {
        GateMember::delete_many()
            .filter(gate_member::Column::GateId.eq(gate_id))
            .filter(gate_member::Column::ActorId.eq(actor_id))
            .exec(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to remove gate member")?;

        tracing::info!(
            gate_id = %gate_id,
            actor_id = %actor_id,
            "Removed gate member"
        );

        Ok(())
    }

    /// List all components in a gate
    pub async fn list_components(&self, gate_id: &str) -> Result<Vec<component::Model>> {
        let components = Component::find()
            .filter(component::Column::GateId.eq(gate_id))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list components")?;

        Ok(components)
    }

    /// Delete a gate
    /// This will cascade delete all members and components
    pub async fn delete(&self, gate_id: &str) -> Result<()> {
        Gate::delete_by_id(gate_id)
            .exec(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to delete gate")?;

        tracing::info!(
            gate_id = %gate_id,
            "Deleted gate"
        );

        Ok(())
    }

    /// List all gate memberships for an actor (across all gates)
    pub async fn list_memberships_for_actor(
        &self,
        actor_id: &str,
    ) -> Result<Vec<gate_member::Model>> {
        let memberships = GateMember::find()
            .filter(gate_member::Column::ActorId.eq(actor_id))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list memberships for actor")?;

        Ok(memberships)
    }

    /// Check if a gate exists
    pub async fn exists(&self, gate_id: &str) -> Result<bool> {
        let exists = Gate::find_by_id(gate_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to check gate existence")?
            .is_some();

        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would go here
    // They require a database connection and Jujutsu manager
}
