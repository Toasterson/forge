use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "source_archive")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub component_id: String,
    pub filename: String,
    pub url: Option<String>,
    pub blob_hash: String,
    pub size_bytes: i64,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::component::Entity",
        from = "Column::ComponentId",
        to = "super::component::Column::Id"
    )]
    Component,
}

impl Related<super::component::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Component.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
