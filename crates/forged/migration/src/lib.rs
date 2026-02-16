pub use sea_orm_migration::prelude::*;

mod m20260120_000001_create_initial_schema;
mod m20260216_000001_add_actor_keys;
mod m20260216_000002_create_build_jobs;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260120_000001_create_initial_schema::Migration),
            Box::new(m20260216_000001_add_actor_keys::Migration),
            Box::new(m20260216_000002_create_build_jobs::Migration),
        ]
    }
}
