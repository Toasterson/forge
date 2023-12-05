//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.6

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "job")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub patch: Option<String>,
    #[sea_orm(column_type = "JsonBinary")]
    pub merge_request_ref: Json,
    #[sea_orm(column_type = "JsonBinary")]
    pub target_ref: Json,
    pub repository: String,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub conf_ref: Option<Json>,
    pub tags: Option<Vec<String>>,
    pub job_type: Option<String>,
    pub package_repo_id: Option<Uuid>,
    pub source_repo_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::job_to_component_record::Entity")]
    JobToComponentRecord,
    #[sea_orm(
        belongs_to = "super::package_repository::Entity",
        from = "Column::PackageRepoId",
        to = "super::package_repository::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    PackageRepository,
    #[sea_orm(
        belongs_to = "super::source_repo::Entity",
        from = "Column::SourceRepoId",
        to = "super::source_repo::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    SourceRepo,
}

impl Related<super::job_to_component_record::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::JobToComponentRecord.def()
    }
}

impl Related<super::package_repository::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PackageRepository.def()
    }
}

impl Related<super::source_repo::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SourceRepo.def()
    }
}

impl Related<super::component::Entity> for Entity {
    fn to() -> RelationDef {
        super::job_to_component_record::Relation::Component.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::job_to_component_record::Relation::Job.def().rev())
    }
}

impl ActiveModelBehavior for ActiveModel {}
