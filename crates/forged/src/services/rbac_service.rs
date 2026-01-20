use crate::entities::{component, gate_member};
use crate::repositories::{ComponentRepository, GateRepository};
use miette::{Context, Result};
use std::sync::Arc;

/// RBAC Service for permission checking
/// Enforces gate and component-level access control
#[derive(Clone)]
pub struct RbacService {
    gate_repo: Arc<GateRepository>,
    component_repo: Arc<ComponentRepository>,
}

impl RbacService {
    pub fn new(gate_repo: Arc<GateRepository>, component_repo: Arc<ComponentRepository>) -> Self {
        Self {
            gate_repo,
            component_repo,
        }
    }

    /// Check if an actor has a specific permission for a gate
    pub async fn check_gate_permission(
        &self,
        actor_id: &str,
        gate_id: &str,
        permission: GatePermission,
    ) -> Result<bool> {
        // 1. Check if actor is the gate owner
        let gate = self
            .gate_repo
            .get_gate(gate_id)
            .await
            .wrap_err("failed to get gate for permission check")?;

        if let Some(g) = gate {
            if g.owner_id == actor_id {
                // Owner has all permissions
                return Ok(true);
            }
        } else {
            // Gate doesn't exist
            return Ok(false);
        }

        // 2. Check gate membership
        let member = self
            .gate_repo
            .get_member(gate_id, actor_id)
            .await
            .wrap_err("failed to get gate member for permission check")?;

        if let Some(m) = member {
            // Check if member has the required permission
            let has_permission = m
                .permissions
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .any(|v| v.as_str() == Some(permission.as_str()))
                })
                .unwrap_or(false);

            Ok(has_permission)
        } else {
            Ok(false)
        }
    }

    /// Check if an actor has a specific permission for a component
    /// This delegates to gate-level permission check since components belong to gates
    pub async fn check_component_permission(
        &self,
        actor_id: &str,
        component_id: &str,
        permission: ComponentPermission,
    ) -> Result<bool> {
        // 1. Get component to find its gate
        let component = self
            .component_repo
            .get_component(component_id)
            .await
            .wrap_err("failed to get component for permission check")?;

        if let Some(c) = component {
            // 2. Check gate permission
            let gate_permission = match permission {
                ComponentPermission::Read => GatePermission::ComponentRead,
                ComponentPermission::Write => GatePermission::ComponentWrite,
            };

            self.check_gate_permission(actor_id, &c.gate_id, gate_permission)
                .await
        } else {
            Ok(false)
        }
    }

    /// Check if an actor can manage gate members (add/remove/update)
    pub async fn check_gate_admin(&self, actor_id: &str, gate_id: &str) -> Result<bool> {
        self.check_gate_permission(actor_id, gate_id, GatePermission::GateAdmin)
            .await
    }

    /// Check if an actor can read gate information
    pub async fn check_gate_read(&self, actor_id: &str, gate_id: &str) -> Result<bool> {
        self.check_gate_permission(actor_id, gate_id, GatePermission::GateRead)
            .await
    }

    /// Check if an actor can modify gate settings
    pub async fn check_gate_write(&self, actor_id: &str, gate_id: &str) -> Result<bool> {
        self.check_gate_permission(actor_id, gate_id, GatePermission::GateWrite)
            .await
    }

    /// Check if an actor can read component data
    pub async fn check_component_read(&self, actor_id: &str, component_id: &str) -> Result<bool> {
        self.check_component_permission(actor_id, component_id, ComponentPermission::Read)
            .await
    }

    /// Check if an actor can modify component data
    pub async fn check_component_write(&self, actor_id: &str, component_id: &str) -> Result<bool> {
        self.check_component_permission(actor_id, component_id, ComponentPermission::Write)
            .await
    }

    /// Get all gates where actor has at least read permission
    pub async fn list_accessible_gates(&self, actor_id: &str) -> Result<Vec<gate_member::Model>> {
        // Get all gates owned by actor
        let owned_gates = self
            .gate_repo
            .list_by_owner(actor_id)
            .await
            .wrap_err("failed to list owned gates")?;

        // Get all gate memberships
        // TODO: This requires a query across all gates - might need optimization
        // For now, we return empty vec as this is not critical for MVP

        Ok(Vec::new())
    }

    /// Get all components in a gate where actor has at least read permission
    pub async fn list_accessible_components(
        &self,
        actor_id: &str,
        gate_id: &str,
    ) -> Result<Vec<component::Model>> {
        // First check if actor has gate read permission
        if !self.check_gate_read(actor_id, gate_id).await? {
            return Ok(Vec::new());
        }

        // If yes, return all components in the gate
        self.component_repo
            .list_by_gate(gate_id)
            .await
            .wrap_err("failed to list components in gate")
    }
}

/// Gate-level permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatePermission {
    GateAdmin,      // Can manage gate members
    GateRead,       // Can view gate info
    GateWrite,      // Can modify gate settings
    ComponentRead,  // Can read components in this gate
    ComponentWrite, // Can modify components in this gate
}

impl GatePermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            GatePermission::GateAdmin => "gate_admin",
            GatePermission::GateRead => "gate_read",
            GatePermission::GateWrite => "gate_write",
            GatePermission::ComponentRead => "component_read",
            GatePermission::ComponentWrite => "component_write",
        }
    }
}

/// Component-level permissions (mapped to gate permissions)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentPermission {
    Read,
    Write,
}

impl ComponentPermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            ComponentPermission::Read => "read",
            ComponentPermission::Write => "write",
        }
    }
}
