use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "blob_metadata")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub hash: String,
    pub blob_type: String,
    pub namespace: String,
    pub fid: String,
    pub size_bytes: i64,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Blob types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlobType {
    SourceArchive,
    Patch,
    License,
    Script,
}

impl BlobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlobType::SourceArchive => "source_archive",
            BlobType::Patch => "patch",
            BlobType::License => "license",
            BlobType::Script => "script",
        }
    }
}

impl std::fmt::Display for BlobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
