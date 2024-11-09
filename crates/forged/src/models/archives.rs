use super::_entities::archives::{ActiveModel, Entity};
use sea_orm::entity::prelude::*;
pub type Archives = Entity;

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}
