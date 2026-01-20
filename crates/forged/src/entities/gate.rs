use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "gate")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub gate_kdl: String,
    pub owner_id: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::actor::Entity",
        from = "Column::OwnerId",
        to = "super::actor::Column::Id"
    )]
    Owner,
    #[sea_orm(has_many = "super::gate_member::Entity")]
    GateMember,
    #[sea_orm(has_many = "super::component::Entity")]
    Component,
}

impl Related<super::actor::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Owner.def()
    }
}

impl Related<super::gate_member::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GateMember.def()
    }
}

impl Related<super::component::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Component.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
