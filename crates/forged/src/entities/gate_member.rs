use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "gate_member")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub gate_id: String,
    pub actor_id: String,
    pub roles: Json,
    pub permissions: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::gate::Entity",
        from = "Column::GateId",
        to = "super::gate::Column::Id"
    )]
    Gate,
    #[sea_orm(
        belongs_to = "super::actor::Entity",
        from = "Column::ActorId",
        to = "super::actor::Column::Id"
    )]
    Actor,
}

impl Related<super::gate::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Gate.def()
    }
}

impl Related<super::actor::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Actor.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

/// Gate roles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Member,
    Viewer,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Viewer => "viewer",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Gate permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    GateAdmin,
    GateRead,
    GateWrite,
    ComponentRead,
    ComponentWrite,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::GateAdmin => "gate_admin",
            Permission::GateRead => "gate_read",
            Permission::GateWrite => "gate_write",
            Permission::ComponentRead => "component_read",
            Permission::ComponentWrite => "component_write",
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
