use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "component_file")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub component_id: String,
    pub kind: String,
    pub name: String,
    pub rel_path: String,
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

/// Component file kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Patch,
    License,
    Script,
}

impl FileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileKind::Patch => "patch",
            FileKind::License => "license",
            FileKind::Script => "script",
        }
    }
}

impl std::fmt::Display for FileKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
