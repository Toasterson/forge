use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use config::{Environment, File};
use deadpool_lapin::lapin::{
    options::{BasicPublishOptions, QueueDeclareOptions},
    protocol::basic::AMQPProperties,
    types::FieldTable,
};
use github::{GitHubError, GitHubEvent, GitHubWebhookRequest};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info};

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
    ci_inbox: String,
}

#[derive(Parser)]
pub struct Args {
    rabbitmq_url: Option<String>,
}

pub fn load_config(args: Args) -> Result<Config> {
    let cfg = config::Config::builder()
        .add_source(File::with_name("/etc/forge/ghwhrecv.yaml").required(false))
        .add_source(
            Environment::with_prefix("WEBHOOK")
                .separator("_")
                .prefix_separator("__"),
        )
        .set_default("listen", "0.0.0.0:3000")?
        .set_default("ci_inbox", "CI_INBOX")?
        .set_override_option("amqp.url", args.rabbitmq_url)?
        .build()?;

    Ok(cfg.try_deserialize()?)
}

#[derive(Debug, Clone)]
struct AppState {
    amqp: deadpool_lapin::Pool,
    ci_inbox: String,
}

pub async fn listen(cfg: Config) -> Result<()> {
    tracing::debug!("Opening RabbitMQ Connection");
    let state = AppState {
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        ci_inbox: cfg.ci_inbox,
    };
    let conn = state.amqp.get().await?;
    tracing::debug!(
        "Connected to {} as {}",
        conn.status().vhost(),
        conn.status().username()
    );

    let channel = conn.create_channel().await?;

    tracing::debug!(
        "Defining inbox: {} queue from channel id {}",
        &state.ci_inbox,
        channel.id()
    );
    channel
        .queue_declare(
            &state.ci_inbox,
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let app = Router::new()
        .route("/", post(handle_webhook))
        .route("/healthz", get(health_check))
        //.layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);
    info!("Listening on {0}", &cfg.listen);
    // run it with hyper on localhost:3000
    axum::Server::bind(&cfg.listen.parse()?)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[derive(Serialize, Default)]
struct HealthResponse {
    amqp_error: Option<String>,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse::default())
}

async fn handle_webhook(State(state): State<AppState>, req: GitHubWebhookRequest) -> Result<()> {
    debug!("Received Webhook: {}", req.get_kind());
    match req.get_event()? {
        GitHubEvent::PullRequest(_) => todo!(),
        GitHubEvent::Issue(_) => todo!(),
        GitHubEvent::IssueComment(_) => todo!(),
        GitHubEvent::Status(_) => todo!(),
        GitHubEvent::Push(event) => {
            info!("Received Push event from Github");
            debug!("event: {:?}", event);
            let conn = state.amqp.get().await?;
            let event_msg = forge::Event::Create(forge::Object::Job(forge::Job {
                before: event.before,
                after: event.after,
                ref_name: event.ref_name,
                repository: event.repository.git_url,
                tags: Some(vec!["push".to_owned()]),
                conf_ref: None,
            }));
            let msg = serde_json::to_vec(&event_msg)?;
            let channel = conn.create_channel().await?;
            channel
                .basic_publish(
                    "",
                    &state.ci_inbox,
                    BasicPublishOptions::default(),
                    &msg,
                    AMQPProperties::default(),
                )
                .await?;
            Ok(())
        }
        GitHubEvent::Ping(_) => todo!(),
    }
}

#[derive(Serialize, Default)]
struct ResturnValue {
    error: Option<String>,
}
