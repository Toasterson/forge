use std::sync::Arc;

use axum::{Json, Router};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use clap::Parser;
use config::Environment;
use deadpool_lapin::lapin::Channel;
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
    QueueDeclareOptions,
};
use deadpool_lapin::lapin::protocol::basic::AMQPProperties;
use deadpool_lapin::lapin::types::FieldTable;
use deadpool_lapin::Pool;
use futures::{join, StreamExt};
use miette::Diagnostic;
use sea_orm::{ActiveValue, Database, DatabaseConnection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, log};

pub use activities::*;
use migration::{Migrator, MigratorTrait};

mod activities;
mod entity;

#[derive(Parser, Debug)]
pub struct Args {
    pub rabbitmq_url: Option<String>,
}

#[derive(Debug, Diagnostic, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    AddrParse(#[from] std::net::AddrParseError),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    #[error(transparent)]
    CreatePool(#[from] deadpool_lapin::CreatePoolError),

    #[error(transparent)]
    Lapin(#[from] deadpool_lapin::lapin::Error),

    #[error(transparent)]
    Pool(#[from] deadpool_lapin::PoolError),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    ParseError(#[from] url::ParseError),

    #[error(transparent)]
    SeaOrm(#[from] sea_orm::DbErr),

    #[error("{0}")]
    String(String),
}

pub type Result<T> = miette::Result<T, Error>;

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            err => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub amqp: deadpool_lapin::Config,
    pub listen: String,
    pub job_inbox: String,
    pub inbox: String,
    pub connection_string: String,
}

pub fn load_config(args: Args) -> Result<Config> {
    let cfg = config::Config::builder()
        .add_source(config::File::with_name("/etc/forge/forged").required(false))
        .add_source(
            Environment::with_prefix("APP")
                .separator("_")
                .prefix_separator("__"),
        )
        .set_default("listen", "0.0.0.0:3100")?
        .set_default("job_inbox", "JOB_INBOX")?
        .set_default("inbox", "INBOX")?
        .set_default(
            "connection_string",
            "postgres://forge:forge@localhost/forge",
        )?
        .set_override_option("amqp.url", args.rabbitmq_url)?
        .build()?;
    Ok(cfg.try_deserialize()?)
}

#[derive(Debug)]
struct AppState {
    amqp: deadpool_lapin::Pool,
    job_inbox: String,
    inbox: String,
    database: DatabaseConnection,
}

type SharedState = Arc<Mutex<AppState>>;

pub async fn listen(cfg: Config) -> Result<()> {
    debug!("Opening Database Connection");
    let mut opts = sea_orm::ConnectOptions::new(cfg.connection_string.clone());
    opts.sqlx_logging_level(log::LevelFilter::Trace)
        .sqlx_logging(true)
        .min_connections(10)
        .max_connections(100);
    debug!("Opening RabbitMQ Connection");
    let state = Arc::new(Mutex::new(AppState {
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        job_inbox: cfg.job_inbox,
        inbox: cfg.inbox,
        database: Database::connect(opts).await?,
    }));
    let conn = state.lock().await.amqp.get().await?;
    let job_inbox = state.lock().await.job_inbox.clone();
    let inbox = state.lock().await.inbox.clone();
    debug!(
        "Connected to {} as {}",
        conn.status().vhost(),
        conn.status().username()
    );

    let channel = conn.create_channel().await?;

    debug!(
        "Defining JOB inbox: {} queue from channel id {}",
        job_inbox,
        channel.id()
    );
    channel
        .queue_declare(
            job_inbox.as_str(),
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    debug!(
        "Defining inbox: {} queue from channel id {}",
        inbox,
        channel.id()
    );
    channel
        .queue_declare(
            inbox.as_str(),
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    debug!("Migrating Database");
    Migrator::up(&state.lock().await.database, None).await?;

    let amqp_consume_pool = state.lock().await.amqp.clone();
    let app = Router::new()
        .route("/healthz", get(health_check))
        //.layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);
    info!("Listening on {0}", &cfg.listen);
    // run it with hyper on localhost:3100
    let _ = join!(
        rabbitmq_listen(
            amqp_consume_pool,
            cfg.connection_string.clone(),
            inbox.as_str(),
            job_inbox.as_str()
        ),
        axum::Server::bind(&cfg.listen.parse()?).serve(app.into_make_service()),
    );
    Ok(())
}

async fn rabbitmq_listen(pool: Pool, connection_string: String, inbox_name: &str, job_inbox_name: &str) -> Result<()> {
    let mut retry_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let database = Database::connect(connection_string).await?;
    loop {
        retry_interval.tick().await;
        info!("connecting amqp consumer...");
        match handle_rabbitmq(pool.clone(), &database, inbox_name, job_inbox_name).await {
            Ok(_) => tracing::info!("rmq listen returned"),
            Err(e) => tracing::error!(error = e.to_string(), "rmq listen had an error"),
        };
    }
}

async fn handle_rabbitmq(
    pool: Pool,
    database: &DatabaseConnection,
    inbox_name: &str,
    job_inbox_name: &str,
) -> Result<()> {
    let rmq_con = pool.get().await.map_err(|e| Error::String(e.to_string()))?;
    let channel = rmq_con.create_channel().await?;
    let job_channel = rmq_con.create_channel().await?;

    let mut consumer = channel
        .basic_consume(
            inbox_name,
            "inbox.consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    info!("amqp consumer connected, waiting for messages");
    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                let tag = delivery.delivery_tag;
                match handle_message(delivery, database, &job_channel, job_inbox_name).await {
                    Ok(_) => {
                        debug!("handled message");
                        channel.basic_ack(tag, BasicAckOptions::default()).await?;
                    }
                    Err(e) => {
                        error!(error = e.to_string(), "failed to handle message");
                        channel.basic_nack(tag, BasicNackOptions::default()).await?;
                    }
                }
            }
            Err(err) => return Err(Error::String(err.to_string())),
        }
    }

    Ok(())
}

async fn handle_message(
    deliver: Delivery,
    database: &DatabaseConnection,
    job_channel: &Channel,
    job_inbox_name: &str,
) -> Result<()> {
    let body = deliver.data;
    let envelope: Event = serde_json::from_slice(&body)?;
    match envelope {
        Event::Create(envelope) => {
            debug!("got create event: {:?}", envelope);
            match envelope.object {
                Object::MergeRequest(mr) => {
                    use sea_orm::entity::prelude::*;
                    let repo = entity::prelude::SourceRepo::find()
                        .filter(entity::source_repo::Column::Url.eq(mr.repository.clone()))
                        .one(database)
                        .await?;
                    if let Some(repo) = repo {
                        debug!("found repo: {:?}", repo);
                        let dbmr = entity::source_merge_request::ActiveModel {
                            id: ActiveValue::Set(Uuid::new_v4()),
                            number: ActiveValue::Set(mr.number as i32),
                            url: ActiveValue::Set(mr.origin_url.unwrap_or(mr.repository.clone())),
                            repository: ActiveValue::Set(repo.id),
                            state: ActiveValue::Set(mr.action),
                            api_kind: ActiveValue::Set("github".to_string()),
                        };
                        dbmr.save(database).await?;
                        let job = Job {
                            patch: mr.patch,
                            ref_name: mr.head.ref_name,
                            base_ref: Some(mr.base.ref_name),
                            repository: mr.repository.clone(),
                            conf_ref: Some("default".to_string()),
                            tags: None,
                            job_type: Some(KnownJobs::CheckForChangedComponents),
                        };
                        let payload = serde_json::to_vec(&job)?;
                        job_channel
                            .basic_publish(
                                "",
                                job_inbox_name,
                                BasicPublishOptions::default(),
                                &payload,
                                AMQPProperties::default(),
                            )
                            .await?;
                    } else {
                        debug!("repo not found, ignoring event");
                    }
                }
                Object::PackageRepository(pr) => {}
                Object::Package(p) => {}
                Object::SoftwareComponent(sc) => {}
                Object::Push(p) => {}
            }
        }
        Event::Update(envelope) => {
            debug!("got update event: {:?}", envelope);
            match envelope.object {
                Object::Push(_) => {}
                Object::MergeRequest(_) => {}
                Object::PackageRepository(_) => {}
                Object::Package(_) => {}
                Object::SoftwareComponent(_) => {}
            }
        }
        Event::Delete(envelope) => {
            debug!("got delete event: {:?}", envelope);
            match envelope.object {
                Object::Push(_) => {}
                Object::MergeRequest(_) => {}
                Object::PackageRepository(_) => {}
                Object::Package(_) => {}
                Object::SoftwareComponent(_) => {}
            }
        }
    }
    Ok(())
}

#[derive(Serialize, Default)]
struct HealthResponse {}

async fn health_check(State(state): State<SharedState>) -> Result<Json<HealthResponse>> {
    let conn = state.lock().await.amqp.get().await?;
    if !matches!(
        conn.status().state(),
        deadpool_lapin::lapin::ConnectionState::Connected
    ) {
        return Err(Error::Lapin(
            deadpool_lapin::lapin::Error::InvalidConnectionState(conn.status().state()),
        ));
    }
    state.lock().await.database.ping().await?;
    Ok(Json(HealthResponse::default()))
}
