use super::_entities::gates::{ActiveModel, Column, Entity, Model};
use loco_rs::prelude::*;

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}

impl Model {
    /// finds a gate by the provided name
    ///
    /// # Errors
    ///
    /// When could not find gate by the given name or DB query error
    pub async fn find_by_name<S: AsRef<str>>(
        db: &DatabaseConnection,
        name: S,
    ) -> ModelResult<Self> {
        let gate = Entity::find()
            .filter(query::condition().eq(Column::Name, name.as_ref()).build())
            .one(db)
            .await?;
        gate.ok_or_else(|| ModelError::EntityNotFound)
    }
}
