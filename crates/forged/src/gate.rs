use crate::rbac::Permission;
use crate::types::{ActorId, ActorKind, ActorRef, GateId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Server-side Gate record that composes the base model from the `gate` crate
/// and enriches it with ownership and membership information for access control.
///
/// Domain clarification:
/// - Actors are Users or Services that perform actions against the server.
/// - Components are software components managed inside gates (not actors).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateRecord {
    /// Stable identifier for this gate within the server.
    pub id: GateId,
    /// The base gate model (from gate crate) describing build DSL and metadata.
    pub base: gate::Gate,
    /// The owner actor. Owner is implied to have all permissions in RBAC.
    pub owner: ActorRef,
    /// Members belonging to this gate with assigned roles and/or direct permissions.
    pub members: Vec<GateMember>,
    /// Creation timestamp (seconds since epoch).
    pub created_at: u64,
    /// Update timestamp (seconds since epoch).
    pub updated_at: u64,
    /// Arbitrary server-side metadata.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateMember {
    pub actor: ActorRef,
    /// Role names assigned to the actor within this gate.
    pub roles: Vec<String>,
    /// Direct permissions granted in addition to roles (fine-grained overrides).
    pub permissions: BTreeSet<Permission>,
}

impl GateRecord {
    pub fn new(id: GateId, base: gate::Gate, owner: ActorRef) -> Self {
        let now = now_sec();
        Self {
            id,
            base,
            owner,
            members: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: None,
        }
    }

    /// Add or replace a member entry for an actor. Owner cannot be added as a member.
    pub fn upsert_member(&mut self, member: GateMember) {
        if member.actor == self.owner {
            return;
        }
        if let Some(existing) = self.members.iter_mut().find(|m| m.actor == member.actor) {
            *existing = member;
        } else {
            self.members.push(member);
        }
        self.touch();
    }

    /// Remove a member by actor.
    pub fn remove_member(&mut self, actor: &ActorRef) {
        self.members.retain(|m| &m.actor != actor);
        self.touch();
    }

    pub fn touch(&mut self) {
        self.updated_at = now_sec();
    }
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::Permission;

    #[test]
    fn serialize_roundtrip() {
        let base = gate::Gate::default();
        let rec = GateRecord::new(
            GateId("gate-1".into()),
            base,
            ActorRef {
                id: ActorId("user-1".into()),
                kind: ActorKind::User,
            },
        );
        let ser = serde_json::to_string_pretty(&rec).unwrap();
        let de: GateRecord = serde_json::from_str(&ser).unwrap();
        assert_eq!(de.id.0, "gate-1");
        assert_eq!(de.owner.id.0, "user-1");
    }

    #[test]
    fn members_management() {
        let base = gate::Gate::default();
        let mut rec = GateRecord::new(
            GateId("g".into()),
            base,
            ActorRef {
                id: ActorId("owner".into()),
                kind: ActorKind::User,
            },
        );
        let m = GateMember {
            actor: ActorRef {
                id: ActorId("svc".into()),
                kind: ActorKind::Service,
            },
            roles: vec!["reader".into()],
            permissions: [Permission::GateRead].into_iter().collect(),
        };
        rec.upsert_member(m);
        assert_eq!(rec.members.len(), 1);
        // owner cannot be added as member
        let owner_member = GateMember {
            actor: rec.owner.clone(),
            roles: vec![],
            permissions: BTreeSet::new(),
        };
        rec.upsert_member(owner_member);
        assert_eq!(rec.members.len(), 1);
    }
}
