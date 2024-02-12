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
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info};

use forge::{ChangeRequest, CommitRef, Scheme};
use github::{GitHubError, GitHubEvent, GitHubWebhookRequest};

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
        .add_source(File::with_name("/etc/forge/ghwhrecv").required(false))
        .add_source(
            Environment::with_prefix("WEBHOOK")
                .separator("_")
                .prefix_separator("__"),
        )
        .set_default("listen", "0.0.0.0:3000")?
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
        GitHubEvent::PullRequest(event) => {
            // Implement a function that creates a PullRequest struct of type forge::PullRequest from
            // the event and sends it to the forge.
            info!("Received PullRequest event from Github");
            debug!("event: {:?}", event);
            let conn = state.amqp.get().await?;
            let event_msg = forge::Event::Create(forge::ActivityEnvelope {
                actor: format!("{}/actors/github", &state.base_url).parse()?,
                to: vec![format!("{}/actors/forge", &state.base_url).parse()?],
                cc: vec![],
                object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                    changes: todo!(),
                    external_ref: todo!(),
                    state: todo!(),
                    contributor: todo!(),
                }),
            });
            debug!("forge event: {:?}", event_msg);
            let msg = serde_json::to_vec(&event_msg)?;
            let channel = conn.create_channel().await?;
            channel
                .basic_publish(
                    "",
                    &state.inbox,
                    BasicPublishOptions::default(),
                    &msg,
                    AMQPProperties::default(),
                )
                .await?;
            Ok(())
        }
        GitHubEvent::Issue(_) => todo!(),
        GitHubEvent::IssueComment(_) => todo!(),
        GitHubEvent::Status(_) => todo!(),
        GitHubEvent::Push(event) => {
            info!("Received Push event from Github");
            debug!("event: {:?}", event);
            let conn = state.amqp.get().await?;
            let event_msg = forge::Event::Create(forge::ActivityEnvelope {
                actor: format!("{}/actors/github", &state.base_url).parse()?,
                to: vec![format!("{}/actors/forge", &state.base_url).parse()?],
                cc: vec![],
                object: forge::ActivityObject::Push(forge::Push {
                    before: event.before,
                    after: event.after,
                    ref_name: event.ref_name,
                    repository: event.repository.git_url,
                }),
            });
            let msg = serde_json::to_vec(&event_msg)?;
            let channel = conn.create_channel().await?;
            channel
                .basic_publish(
                    "",
                    &state.inbox,
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
struct ReturnValue {
    error: Option<String>,
}
