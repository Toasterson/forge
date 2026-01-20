use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "component")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub gate_id: String,
    pub name: String,
    pub recipe_kdl: String,
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
    #[sea_orm(has_many = "super::source_archive::Entity")]
    SourceArchive,
    #[sea_orm(has_many = "super::component_file::Entity")]
    ComponentFile,
}

impl Related<super::gate::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Gate.def()
    }
}

impl Related<super::source_archive::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceArchive.def()
    }
}

impl Related<super::component_file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ComponentFile.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
