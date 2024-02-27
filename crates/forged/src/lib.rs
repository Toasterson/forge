use crate::graphql::{mutation::MutationRoot, query::QueryRoot};

use async_graphql::{EmptySubscription, Schema};
use async_graphql_axum::GraphQL;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use config::Environment;
use deadpool_lapin::lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueDeclareOptions,
};
use deadpool_lapin::lapin::types::FieldTable;
use deadpool_lapin::Pool;
use futures::{join, StreamExt};
use message_queue::handle_message;
use miette::Diagnostic;
use prisma::PrismaClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

mod graphql;
mod message_queue;
#[allow(warnings, unused)]
mod prisma;

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
    NewClient(#[from] prisma_client_rust::NewClientError),

    #[error("{0}")]
    String(String),

    #[error("entity not found {0}")]
    NotFound(String),
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
        .set_default("amqp.url", "amqp://dev:dev@localhost:5672/master")?
        .set_override_option("amqp.url", args.rabbitmq_url)?
        .build()?;
    Ok(cfg.try_deserialize()?)
}

#[allow(dead_code)]
#[derive(Debug)]
struct AppState {
    amqp: Pool,
    job_inbox: String,
    inbox: String,
    prisma: PrismaClient,
    graphql: GraphQLConfig,
}

type SharedState = Arc<Mutex<AppState>>;

pub async fn listen(cfg: Config) -> Result<()> {
    debug!("Opening Database Connection");
    let db_conn = prisma::PrismaClient::_builder()
        .with_url(cfg.connection_string.clone())
        .build()
        .await?;
    debug!("Opening RabbitMQ Connection");
    let state = Arc::new(Mutex::new(AppState {
        graphql: cfg.graphql.clone(),
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        job_inbox: cfg.job_inbox,
        inbox: cfg.inbox,
        prisma: db_conn,
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

    let schema = Schema::build(
        QueryRoot::default(),
        MutationRoot::default(),
        EmptySubscription,
    )
    .data(state.clone())
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
    let database = PrismaClient::_builder()
        .with_url(connection_string)
        .build()
        .await?;
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
    database: &PrismaClient,
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
    Ok(Json(HealthResponse::default()))
}
