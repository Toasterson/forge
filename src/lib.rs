use std::sync::Arc;

use async_graphql::{EmptySubscription, Schema};
use async_graphql_axum::GraphQL;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use config::Environment;
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
    QueueDeclareOptions,
};
use deadpool_lapin::lapin::protocol::basic::AMQPProperties;
use deadpool_lapin::lapin::types::FieldTable;
use deadpool_lapin::lapin::Channel;
use deadpool_lapin::Pool;
use futures::{join, StreamExt};
use miette::Diagnostic;
use sea_orm::{ActiveValue, Database, DatabaseConnection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, log, trace};

use crate::graphql::{MutationRoot, QueryRoot};
pub use activities::*;
use migration::{Migrator, MigratorTrait};

mod activities;
mod entity;
mod graphql;

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
    pub graphql: GraphQLConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GraphQLConfig {
    pub depth_limit: usize,
    pub complexity_limit: usize,
    pub use_ssl: bool,
    pub domain: String,
    pub port: u16,
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
        .set_default("graphql.depth_limit", 10)?
        .set_default("graphql.complexity_limit", 1000)?
        .set_default("graphql.use_ssl", false)?
        .set_default("graphql.domain", "localhost")?
        .set_default("graphql.port", 3100)?
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
    amqp: Pool,
    job_inbox: String,
    inbox: String,
    database: DatabaseConnection,
    graphql: GraphQLConfig,
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
        graphql: cfg.graphql.clone(),
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        job_inbox: cfg.job_inbox,
        inbox: cfg.inbox,
        database: Database::connect(opts.clone()).await?,
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

    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(Database::connect(opts.clone()).await?)
        .finish();

    let amqp_consume_pool = state.lock().await.amqp.clone();
    let app = Router::new()
        .route("/healthz", get(health_check))
        .route("/", get(playground).post_service(GraphQL::new(schema)))
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

async fn rabbitmq_listen(
    pool: Pool,
    connection_string: String,
    inbox_name: &str,
    job_inbox_name: &str,
) -> Result<()> {
    let mut retry_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let database = Database::connect(connection_string).await?;
    loop {
        retry_interval.tick().await;
        info!("connecting amqp consumer...");
        match handle_rabbitmq(pool.clone(), &database, inbox_name, job_inbox_name).await {
            Ok(_) => info!("rmq listen returned"),
            Err(e) => error!(error = e.to_string(), "rmq listen had an error"),
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
                ActivityObject::MergeRequest(mr) => {
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
                            target_ref: ActiveValue::Set(serde_json::to_value(&mr.target_ref)?),
                            merge_request_ref: ActiveValue::Set(serde_json::to_value(
                                &mr.merge_request_ref,
                            )?),
                        };
                        let res = dbmr.insert(database).await?;
                        trace!("saved merge request: {:?}", res);
                        let job = Job {
                            patch: mr.patch,
                            merge_request_ref: mr.merge_request_ref.clone(),
                            target_ref: mr.target_ref.clone(),
                            repository: mr.repository.clone(),
                            conf_ref: Some("default".to_string()),
                            tags: None,
                            job_type: Some(KnownJobs::CheckForChangedComponents),
                            merge_request_id: Some(res.id.clone()),
                        };
                        let db_job = entity::job::ActiveModel {
                            id: ActiveValue::Set(Uuid::new_v4()),
                            patch: ActiveValue::Set(job.patch.clone().map(|u| u.to_string())),
                            merge_request_ref: ActiveValue::Set(serde_json::to_value(
                                &job.merge_request_ref,
                            )?),
                            target_ref: ActiveValue::Set(serde_json::to_value(&job.target_ref)?),
                            repository: ActiveValue::Set(job.repository.clone()),
                            conf_ref: ActiveValue::Set(None),
                            tags: ActiveValue::Set(job.tags.clone()),
                            job_type: ActiveValue::NotSet,
                            package_repo_id: ActiveValue::NotSet,
                            source_repo_id: ActiveValue::Set(repo.id.clone()),
                        };
                        db_job.insert(database).await?;

                        trace!("sending job: {:?}", job);
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
                        //TODO Dispatch job to self to create package repository for the merge request
                    } else {
                        debug!("repo not found, ignoring event");
                    }
                }
                ActivityObject::PackageRepository(pr) => {
                    use sea_orm::entity::prelude::*;
                    let dbpr = entity::prelude::PackageRepository::find()
                        .filter(
                            entity::package_repository::Column::Url.eq(pr.url.clone().to_string()),
                        )
                        .one(database)
                        .await?;
                    if dbpr.is_none() {
                        let new_repo_id = Uuid::new_v4();
                        let dbpr = entity::package_repository::ActiveModel {
                            id: ActiveValue::Set(new_repo_id.clone()),
                            name: ActiveValue::Set(pr.name.clone()),
                            url: ActiveValue::Set(pr.url.clone().to_string()),
                            public_key: ActiveValue::Set(pr.public_key.clone()),
                        };
                        dbpr.save(database).await?;
                        for publisher in pr.publishers {
                            let dbp = entity::publisher::ActiveModel {
                                id: ActiveValue::Set(Uuid::new_v4()),
                                name: ActiveValue::Set(publisher),
                                package_repository_id: ActiveValue::Set(new_repo_id.clone()),
                            };
                            dbp.save(database).await?;
                        }
                        //TODO Dispatch worker to create the repository (merge as function with merge request creation.)
                    } else {
                        debug!("repo found, ignoring event");
                    }
                }
                ActivityObject::Package(_) => {
                    //TODO save built packages to db and dispatch jobs to add them to the repo.
                    //TODO put messages where we do not have a repository into the wait queue.
                }
                ActivityObject::SoftwareComponent(sc) => {
                    //TODO save software components to DB and dispatch builds
                }
                ActivityObject::Push(_) => {
                    //TODO dispatch builds
                }
            }
        }
        Event::Update(envelope) => {
            debug!("got update event: {:?}", envelope);
            match envelope.object {
                ActivityObject::Push(_) => {
                    //TODO make some grave message that we wont do this and bounce message
                }
                ActivityObject::MergeRequest(_) => {
                    //TODO dispatch db update and check if we need to start some new builds
                }
                ActivityObject::PackageRepository(_) => {
                    //TODO dispatch worker to make repo
                }
                ActivityObject::Package(_) => {
                    //TODO make a grave error that this is not supported.
                }
                ActivityObject::SoftwareComponent(_) => {
                    //TODO Bump Revision and trigger builds
                }
            }
        }
        Event::Delete(envelope) => {
            debug!("got delete event: {:?}", envelope);
            match envelope.object {
                ActivityObject::Push(_) => {
                    //TODO not supported
                }
                ActivityObject::MergeRequest(_) => {
                    //TODO Close MR
                }
                ActivityObject::PackageRepository(_) => {
                    //TODO dispatch cleanup
                }
                ActivityObject::Package(_) => {
                    //TODO dispatch cleanup
                }
                ActivityObject::SoftwareComponent(_) => {
                    //TODO dispatch cleanup
                }
            }
        }
    }
    Ok(())
}
async fn playground(State(state): State<SharedState>) -> impl IntoResponse {
    use async_graphql::http::*;
    let scheme = if state.lock().await.graphql.use_ssl {
        "https"
    } else {
        "http"
    };
    let domain = state.lock().await.graphql.domain.clone();
    let port = state.lock().await.graphql.port.clone();
    let endpoint = format!("{}://{}:{}", scheme, domain, port);
    Html(playground_source(GraphQLPlaygroundConfig::new(
        endpoint.as_str(),
    )))
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
