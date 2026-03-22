use crate::entities::{component, gate, gate_member};
use crate::repositories::{ComponentRepository, GateRepository};
use crate::services::{RbacService, ServerPermission};
use miette::{Context, Result};
use std::sync::Arc;

/// High-level gate management with RBAC enforcement
#[derive(Clone)]
pub struct GateManager {
    gate_repo: Arc<GateRepository>,
    component_repo: Arc<ComponentRepository>,
    rbac: Arc<RbacService>,
}

impl GateManager {
    pub fn new(
        gate_repo: Arc<GateRepository>,
        component_repo: Arc<ComponentRepository>,
        rbac: Arc<RbacService>,
    ) -> Self {
        Self {
            gate_repo,
            component_repo,
            rbac,
        }
    }

    /// Create a new gate
    /// The creator automatically becomes the owner with all permissions
    pub async fn create_gate(
        &self,
        actor_id: &str,
        name: String,
        gate_kdl: String,
    ) -> Result<gate::Model> {
        // Check server-level permission to create gates
        let can_create = self
            .rbac
            .check_server_permission(actor_id, ServerPermission::GateCreate)
            .await?;
        if !can_create {
            return Err(miette::miette!(
                "You do not have permission to create gates.\n\
                 Ask a server administrator to grant you the gate_create permission."
            ));
        }

        // Validate gate KDL before creating
        validate_gate_kdl(&name, &gate_kdl)?;

        let gate = self
            .gate_repo
            .create_gate(name, gate_kdl, actor_id.to_string())
            .await
            .wrap_err("failed to create gate")?;

        tracing::info!(
            gate_id = %gate.id,
            owner_id = %actor_id,
            "Gate created"
        );

        Ok(gate)
    }

    /// Get gate details
    /// Requires GateRead permission
    pub async fn get_gate(&self, actor_id: &str, gate_id: &str) -> Result<gate::Model> {
        // Check permission
        if !self.rbac.check_gate_read(actor_id, gate_id).await? {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have read access to gate {}. \n\
                 Request access from the gate owner or an administrator.",
                actor_id,
                gate_id
            ));
        }

        let gate = self
            .gate_repo
            .get_gate(gate_id)
            .await
            .wrap_err("failed to get gate")?
            .ok_or_else(|| miette::miette!("Gate not found: {}", gate_id))?;

        Ok(gate)
    }

    /// Update gate KDL
    /// Requires GateWrite permission
    pub async fn update_gate(
        &self,
        actor_id: &str,
        gate_id: &str,
        gate_kdl: String,
    ) -> Result<gate::Model> {
        // Check permission
        if !self.rbac.check_gate_write(actor_id, gate_id).await? {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have write access to gate {}. \n\
                 Only gate administrators can update gate settings.",
                actor_id,
                gate_id
            ));
        }

        // Validate gate KDL
        validate_gate_kdl("gate.kdl", &gate_kdl)?;

        let gate = self
            .gate_repo
            .update_gate(gate_id, gate_kdl)
            .await
            .wrap_err("failed to update gate")?;

        tracing::info!(
            gate_id = %gate_id,
            updated_by = %actor_id,
            "Gate updated"
        );

        Ok(gate)
    }

    /// Add a member to a gate
    /// Requires GateAdmin permission
    pub async fn add_member(
        &self,
        actor_id: &str,
        gate_id: &str,
        member_actor_id: &str,
        roles: Vec<String>,
        permissions: Vec<String>,
    ) -> Result<gate_member::Model> {
        // Check permission
        if !self.rbac.check_gate_admin(actor_id, gate_id).await? {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have admin access to gate {}. \n\
                 Only gate owners and administrators can manage members.",
                actor_id,
                gate_id
            ));
        }

        // Validate permissions
        for perm in &permissions {
            if !Self::is_valid_permission(perm) {
                return Err(miette::miette!(
                    "Invalid permission: {}. \n\
                     Valid permissions: gate_admin, gate_read, gate_write, component_read, component_write",
                    perm
                ));
            }
        }

        let member = self
            .gate_repo
            .add_member(gate_id, member_actor_id, roles, permissions)
            .await
            .wrap_err("failed to add gate member")?;

        tracing::info!(
            gate_id = %gate_id,
            member_id = %member_actor_id,
            added_by = %actor_id,
            "Gate member added"
        );

        Ok(member)
    }

    /// Remove a member from a gate
    /// Requires GateAdmin permission
    /// Cannot remove the gate owner
    pub async fn remove_member(
        &self,
        actor_id: &str,
        gate_id: &str,
        member_actor_id: &str,
    ) -> Result<()> {
        // Check permission
        if !self.rbac.check_gate_admin(actor_id, gate_id).await? {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have admin access to gate {}",
                actor_id,
                gate_id
            ));
        }

        // Verify not removing the owner
        let gate = self
            .gate_repo
            .get_gate(gate_id)
            .await?
            .ok_or_else(|| miette::miette!("Gate not found: {}", gate_id))?;

        if gate.owner_id == member_actor_id {
            return Err(miette::miette!(
                "Cannot remove gate owner as a member. \n\
                 Transfer ownership first if you want to change the owner."
            ));
        }

        self.gate_repo
            .remove_member(gate_id, member_actor_id)
            .await
            .wrap_err("failed to remove gate member")?;

        tracing::info!(
            gate_id = %gate_id,
            member_id = %member_actor_id,
            removed_by = %actor_id,
            "Gate member removed"
        );

        Ok(())
    }

    /// List all members of a gate
    /// Requires GateRead permission
    pub async fn list_members(
        &self,
        actor_id: &str,
        gate_id: &str,
    ) -> Result<Vec<gate_member::Model>> {
        // Check permission
        if !self.rbac.check_gate_read(actor_id, gate_id).await? {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have read access to gate {}",
                actor_id,
                gate_id
            ));
        }

        let members = self
            .gate_repo
            .list_members(gate_id)
            .await
            .wrap_err("failed to list gate members")?;

        Ok(members)
    }

    /// List all components in a gate
    /// Requires GateRead permission
    pub async fn list_components(
        &self,
        actor_id: &str,
        gate_id: &str,
    ) -> Result<Vec<component::Model>> {
        // Check permission
        if !self.rbac.check_gate_read(actor_id, gate_id).await? {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have read access to gate {}",
                actor_id,
                gate_id
            ));
        }

        let components = self
            .component_repo
            .list_by_gate(gate_id)
            .await
            .wrap_err("failed to list components")?;

        Ok(components)
    }

    /// List all gates owned by an actor
    pub async fn list_owned_gates(&self, actor_id: &str) -> Result<Vec<gate::Model>> {
        let gates = self
            .gate_repo
            .list_by_owner(actor_id)
            .await
            .wrap_err("failed to list owned gates")?;

        Ok(gates)
    }

    /// List all gates (for admin/system use)
    pub async fn list_all_gates(&self) -> Result<Vec<gate::Model>> {
        let gates = self
            .gate_repo
            .list_all()
            .await
            .wrap_err("failed to list all gates")?;

        Ok(gates)
    }

    /// Validate permission string
    fn is_valid_permission(perm: &str) -> bool {
        matches!(
            perm,
            "gate_admin" | "gate_read" | "gate_write" | "component_read" | "component_write"
        )
    }
}

/// Validate gate KDL by parsing it through the gate crate's parser.
fn validate_gate_kdl(name: &str, kdl_content: &str) -> Result<()> {
    knuffel::parse::<::gate::Gate>(name, kdl_content).map_err(|e| {
        miette::miette!(
            "Invalid gate KDL: {}\n\
             Check the gate definition syntax.\n\
             Required fields: name, version, branch, publisher.\n\
             Example:\n  \
               name \"my-gate\"\n  \
               version \"0.5.11\"\n  \
               branch \"2024.0.0\"\n  \
               publisher \"myorg\"",
            e
        )
    })?;
    Ok(())
}
