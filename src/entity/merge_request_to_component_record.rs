//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.6

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "merge_request_to_component_record")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub merge_request_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub component_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
