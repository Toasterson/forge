use axum::{
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use clap::Parser;
use config::{Environment, File};
use deadpool_lapin::lapin::{Channel, options::QueueDeclareOptions, types::FieldTable};
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions};
use deadpool_lapin::Pool;
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info};
use futures::{join, StreamExt};
use forge::{Job, Scheme};
use github::GitHubError;

#[derive(Error, Diagnostic, Debug)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    AddrParse(#[from] std::net::AddrParseError),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    #[error(transparent)]
    GitHubError(#[from] GitHubError),

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

    #[error("{0}")]
    String(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::GitHubError(err) => err.into_response(),
            err => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                err.to_string(),
            )
                .into_response(),
        }
    }
}

type Result<T> = miette::Result<T, Error>;

#[derive(Deserialize, Serialize)]
pub struct Config {
    amqp: deadpool_lapin::Config,
    listen: String,
    job_inbox: String,
    inbox: String,
    domain: String,
    scheme: String,
}

#[derive(Parser)]
pub struct Args {
    rabbitmq_url: Option<String>,
}

pub fn load_config(args: Args) -> Result<Config> {
    let cfg = config::Config::builder()
        .add_source(File::with_name("/etc/forge/worker").required(false))
        .add_source(
            Environment::with_prefix("WORKER")
                .separator("_")
                .prefix_separator("__"),
        )
        .set_default("listen", "0.0.0.0:3101")?
        .set_default("job_inbox", "JOB_INBOX")?
        .set_default("inbox", "INBOX")?
        .set_default("scheme", Scheme::HTTPS.to_string())?
        .set_override_option("amqp.url", args.rabbitmq_url)?
        .build()?;

    Ok(cfg.try_deserialize()?)
}

#[derive(Debug, Clone)]
struct AppState {
    amqp: deadpool_lapin::Pool,
    inbox: String,
    base_url: String,
}

pub async fn listen(cfg: Config) -> Result<()> {
    debug!("Opening RabbitMQ Connection");
    let state = AppState {
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        inbox: cfg.inbox,
        base_url: format!("{}://{}", Scheme::from(cfg.scheme), cfg.domain),
    };
    let conn = state.amqp.get().await?;
    debug!(
        "Connected to {} as {}",
        conn.status().vhost(),
        conn.status().username()
    );

    let channel = conn.create_channel().await?;

    debug!(
        "Defining inbox: {} queue from channel id {}",
        &state.inbox,
        channel.id()
    );
    channel
        .queue_declare(
            &state.inbox,
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let app = Router::new()
        .route("/healthz", get(health_check))
        //.layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);
    info!("Listening on {0}", &cfg.listen);
    // run it with hyper on localhost:3000
        let _ = join!(
        rabbitmq_listen(
            amqp_consume_pool,
            job_inbox.as_str(),
            inbox.as_str()
        ),
        axum::Server::bind(&cfg.listen.parse()?).serve(app.into_make_service()),
    );
    Ok(())
}

#[derive(Serialize, Default)]
struct HealthResponse {
    amqp_error: Option<String>,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse::default())
}

async fn rabbitmq_listen(
    pool: Pool,
    job_inbox_name: &str,
    inbox_name: &str,
) -> Result<()> {
    let mut retry_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    loop {
        retry_interval.tick().await;
        info!("connecting amqp consumer...");
        match handle_rabbitmq(pool.clone(), job_inbox_name, inbox_name).await {
            Ok(_) => info!("rmq listen returned"),
            Err(e) => error!(error = e.to_string(), "rmq listen had an error"),
        };
    }
}

async fn handle_rabbitmq(
    pool: Pool,
    job_inbox_name: &str,
    inbox_name: &str,
) -> Result<()> {
    let rmq_con = pool.get().await.map_err(|e| forge::Error::String(e.to_string()))?;
    let channel = rmq_con.create_channel().await?;

    let mut consumer = channel
        .basic_consume(
            job_inbox_name,
            "worker.consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    info!("amqp consumer connected, waiting for messages");
    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                let tag = delivery.delivery_tag;
                match handle_message(delivery, &channel, inbox_name).await {
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
    channel: &Channel,
    inbox_name: &str,
) -> Result<()> {
    let body = deliver.data;
    let job: Job = serde_json::from_slice(&body)?;
    if let Some(know_job) = job.job_type {

    } else {

    }
    Ok(())
}