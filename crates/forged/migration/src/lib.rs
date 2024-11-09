#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20220101_000001_users;
mod m20231103_114510_notes;

mod m20241012_120305_gates;
mod m20241109_194935_components;
mod m20241109_195401_archives;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20231103_114510_notes::Migration),
            Box::new(m20241012_120305_gates::Migration),
            Box::new(m20241109_194935_components::Migration),
            Box::new(m20241109_195401_archives::Migration),
        ]
    }
}
