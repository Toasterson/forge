use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "actor")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub kind: String,
    pub oidc_sub: Option<String>,
    pub display_name: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::gate::Entity")]
    Gate,
    #[sea_orm(has_many = "super::gate_member::Entity")]
    GateMember,
    #[sea_orm(has_many = "super::operation::Entity")]
    Operation,
}

impl Related<super::gate::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Gate.def()
    }
}

impl Related<super::gate_member::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GateMember.def()
    }
}

impl Related<super::operation::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Operation.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

/// Actor kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Service,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::Service => "service",
        }
    }
}

impl std::fmt::Display for ActorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
