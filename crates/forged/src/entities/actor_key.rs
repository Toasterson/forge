use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "actor_key")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub actor_id: String,
    pub key_id: String,
    pub algorithm: String,
    /// Stored as the full OpenSSH public key string (e.g. "ssh-ed25519 AAAA...")
    pub public_key: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::actor::Entity",
        from = "Column::ActorId",
        to = "super::actor::Column::Id"
    )]
    Actor,
}

impl Related<super::actor::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Actor.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
