use crate::{
    controllers,
    models::_entities::{notes, users},
    tasks,
};
use async_trait::async_trait;
use loco_rs::prelude::*;
use loco_rs::storage::drivers::object_store_adapter::ObjectStoreAdapter;
use loco_rs::storage::drivers::StoreDriver;
use loco_rs::storage::{drivers, Storage};
use loco_rs::{
    app::{AppContext, Hooks},
    boot::{create_app, BootResult, StartMode},
    controller::AppRoutes,
    db::{self, truncate_table},
    environment::Environment,
    task::Tasks,
    Result,
};
use migration::Migrator;
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct S3Config {
    region: String,
    bucket: String,
    endpoint: String,
    access_key: String,
    secret_key: String,
    virtual_hosted_style_request: bool,
    insecure: bool,
}

fn new_s3_driver(s3_config: S3Config) -> std::result::Result<Box<dyn StoreDriver>, Error> {
    let s3 = object_store::aws::AmazonS3Builder::new()
        .with_endpoint(s3_config.endpoint)
        .with_access_key_id(s3_config.access_key)
        .with_secret_access_key(s3_config.secret_key)
        .with_region(s3_config.region.clone())
        .with_bucket_name(s3_config.bucket.clone())
        .with_virtual_hosted_style_request(s3_config.virtual_hosted_style_request)
        .with_client_options(
            object_store::ClientOptions::new().with_allow_invalid_certificates(s3_config.insecure),
        )
        .build()
        .map_err(Box::from)?;

    Ok(Box::new(ObjectStoreAdapter::new(Box::new(s3))))
}

#[derive(Debug, Deserialize)]
struct LocalStorageConfig {
    path: String,
}

pub struct App;
#[async_trait]
impl Hooks for App {
    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    async fn boot(mode: StartMode, environment: &Environment) -> Result<BootResult> {
        create_app::<Self, Migrator>(mode, environment).await
    }

    fn routes(_ctx: &AppContext) -> AppRoutes {
        AppRoutes::with_default_routes()
            .prefix("/api/v1")
            .add_route(controllers::archives::routes())
            .add_route(controllers::components::routes())
            .add_route(controllers::gates::routes())
            .add_route(controllers::auth::routes())
            .add_route(controllers::user::routes())
    }

    async fn after_context(ctx: AppContext) -> Result<AppContext> {
        let mut ctx = ctx;
        if let Some(init) = &ctx.config.initializers {
            for (name, value) in init {
                match name.as_str() {
                    "s3" => {
                        let s3_config: S3Config = serde_json::from_value(value.clone())?;
                        let s3_driver = new_s3_driver(s3_config)?;
                        ctx.storage = Storage::single(s3_driver).into();
                    }
                    "local_storage" => {
                        let local_storage: LocalStorageConfig =
                            serde_json::from_value(value.clone())?;
                        let local_driver = drivers::local::new_with_prefix(local_storage.path)?;
                        ctx.storage = Storage::single(local_driver).into();
                    }
                    &_ => {}
                }
            }
        }
        Ok(ctx)
    }

    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue
            .register(crate::workers::archive_fetcher::ArchiveFetcherWorker::build(ctx))
            .await?;
        queue
            .register(crate::workers::report_worker::Worker::build(ctx))
            .await?;
        Ok(())
    }

    fn register_tasks(tasks: &mut Tasks) {
        tasks.register(tasks::seed::SeedData);
    }

    async fn truncate(db: &DatabaseConnection) -> Result<()> {
        truncate_table(db, users::Entity).await?;
        truncate_table(db, notes::Entity).await?;
        Ok(())
    }

    async fn seed(db: &DatabaseConnection, base: &Path) -> Result<()> {
        db::seed::<users::ActiveModel>(db, &base.join("users.yaml").display().to_string()).await?;
        db::seed::<notes::ActiveModel>(db, &base.join("notes.yaml").display().to_string()).await?;
        Ok(())
    }
}
