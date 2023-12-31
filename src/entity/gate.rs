//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.6

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "gate")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub r#ref: String,
    pub branch: String,
    pub publisher_id: Uuid,
    pub transforms: Json,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::component::Entity")]
    Component,
    #[sea_orm(
        belongs_to = "super::publisher::Entity",
        from = "Column::PublisherId",
        to = "super::publisher::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Publisher,
    #[sea_orm(has_many = "super::source_to_gate_record::Entity")]
    SourceToGateRecord,
}

impl Related<super::component::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Component.def()
    }
}

impl Related<super::publisher::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Publisher.def()
    }
}

impl Related<super::source_to_gate_record::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceToGateRecord.def()
    }
}

impl Related<super::source_repo::Entity> for Entity {
    fn to() -> RelationDef {
        super::source_to_gate_record::Relation::SourceRepo.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::source_to_gate_record::Relation::Gate.def().rev())
    }
}

impl ActiveModelBehavior for ActiveModel {}
