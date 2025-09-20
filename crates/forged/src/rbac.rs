use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    // Gates
    GateRead,
    GateWrite,
    GateAdmin,
    // Components
    ComponentRead,
    ComponentWrite,
    ComponentAdmin,
    // Users / Actors
    ActorRead,
    ActorWrite,
    // Sessions
    SessionManage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub permissions: BTreeSet<Permission>,
}

impl Role {
    pub fn new<N: Into<String>>(
        name: N,
        permissions: impl IntoIterator<Item = Permission>,
    ) -> Self {
        Self {
            name: name.into(),
            permissions: permissions.into_iter().collect(),
        }
    }

    pub fn contains(&self, perm: &Permission) -> bool {
        self.permissions.contains(perm)
    }
}

// Default roles
pub mod defaults {
    use super::{Permission::*, Role};

    pub fn owner() -> Role {
        Role::new(
            "owner",
            [
                GateRead,
                GateWrite,
                GateAdmin,
                ComponentRead,
                ComponentWrite,
                ComponentAdmin,
                ActorRead,
                ActorWrite,
                SessionManage,
            ],
        )
    }

    pub fn reader() -> Role {
        Role::new("reader", [GateRead, ComponentRead, ActorRead])
    }

    pub fn contributor() -> Role {
        Role::new(
            "contributor",
            [
                GateRead,
                ComponentRead,
                ActorRead,
                ComponentWrite,
                GateWrite,
            ],
        )
    }

    pub fn admin() -> Role {
        Role::new(
            "admin",
            [
                GateRead,
                GateWrite,
                GateAdmin,
                ComponentRead,
                ComponentWrite,
                ComponentAdmin,
                ActorRead,
                ActorWrite,
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_has_all() {
        let owner = defaults::owner();
        assert!(owner.contains(&Permission::GateAdmin));
        assert!(owner.contains(&Permission::SessionManage));
    }
}
