use super::_entities::components::{ActiveModel, Entity};
use sea_orm::entity::prelude::*;
pub type Components = Entity;

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}
