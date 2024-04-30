use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use config::{Environment, File};
use deadpool_lapin::lapin::{
    options::{BasicPublishOptions, ExchangeDeclareOptions},
    protocol::basic::AMQPProperties,
    types::FieldTable,
};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::{debug, error, event, info, instrument, span, Level};

use forge::{
    build_public_id, ChangeRequest, ChangeRequestState, CommitRef, IdKind, Label, Milestone, Scheme,
};
use github::{GitHubError, GitHubEvent, GitHubWebhookRequest, PullRequestPayloadSharedFields};
use url::Url;

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

    #[error(transparent)]
    IOError(#[from] std::io::Error),
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
        .set_default("amqp.url", "amqp://dev:dev@localhost:5672/master")?
        .set_override_option("amqp.url", args.rabbitmq_url)?
        .build()?;

    Ok(cfg.try_deserialize()?)
}

#[derive(Debug, Clone)]
struct AppState {
    amqp: deadpool_lapin::Pool,
    inbox: String,
    job_inbox: String,
    base_url: Url,
}

pub async fn listen(cfg: Config) -> Result<()> {
    debug!("Opening RabbitMQ Connection");
    let state = AppState {
        amqp: cfg
            .amqp
            .create_pool(Some(deadpool_lapin::Runtime::Tokio1))?,
        inbox: cfg.inbox,
        job_inbox: cfg.job_inbox,
        base_url: format!("{}://{}", Scheme::from(cfg.scheme), cfg.domain).parse()?,
    };
    let conn = state.amqp.get().await?;
    debug!(
        "Connected to {} as {}",
        conn.status().vhost(),
        conn.status().username()
    );

    let channel = conn.create_channel().await?;

    debug!(
        "Defining inbox: {} exchange from channel id {}",
        &state.inbox,
        channel.id()
    );
    channel
        .exchange_declare(
            &state.inbox,
            deadpool_lapin::lapin::ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;
    debug!(
        "Defining job exchange: {} from channel id {}",
        &state.job_inbox,
        channel.id()
    );

    channel
        .exchange_declare(
            &state.job_inbox,
            deadpool_lapin::lapin::ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
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
    let listener = TcpListener::bind(&cfg.listen).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

#[derive(Serialize, Default)]
struct HealthResponse {
    amqp_error: Option<String>,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse::default())
}

fn build_change_request(
    shared: PullRequestPayloadSharedFields,
    is_merge_event: bool,
    is_set_to_draft_event: bool,
) -> Result<ChangeRequest> {
    Ok(ChangeRequest {
        id: shared.pull_request.id.to_string(),
        title: shared.pull_request.title,
        body: shared.pull_request.body.unwrap_or(String::new()),
        changes: vec![],
        external_ref: forge::ExternalReference::GitHub {
            pull_request: shared.pull_request.url,
        },
        state: if shared.pull_request.draft || is_set_to_draft_event {
            ChangeRequestState::Draft
        } else if shared.pull_request.merged || is_merge_event {
            ChangeRequestState::Applied
        } else {
            match shared.pull_request.state {
                github::PullRequestState::Open => ChangeRequestState::Open,
                github::PullRequestState::Closed => ChangeRequestState::Closed,
            }
        },
        contributor: format!(
            "{}@github.com",
            shared.sender.login.unwrap_or(String::from("noreply"))
        ),
        labels: shared
            .pull_request
            .labels
            .into_iter()
            .map(|l| Label {
                name: l.name,
                description: l.description,
                color: l.color,
            })
            .collect(),
        milestone: shared.pull_request.milestone.map(|m| Milestone {
            number: m.number,
            title: m.title,
            description: m.description,
        }),
        git_url: shared.repository.ssh_url,
        head: CommitRef {
            sha: shared.pull_request.head.sha,
            ref_name: shared.pull_request.head.ref_name,
        },
        base: CommitRef {
            sha: shared.pull_request.base.sha,
            ref_name: shared.pull_request.base.ref_name,
        },
    })
}

#[instrument(level = "trace", skip_all)]
async fn handle_webhook(State(state): State<AppState>, req: GitHubWebhookRequest) -> Result<()> {
    debug!("Received Webhook: {}", req.get_kind());
    match req.get_event()? {
        GitHubEvent::PullRequest(event) => {
            let span = span!(Level::DEBUG, "PullRequest match arm");
            let _enter = span.enter();
            event!(Level::INFO, pr = ?event.clone(), "Received pull request event");
            let from_actor: Url = build_public_id(IdKind::Actor, &state.base_url, "", "github")?;
            let to_actor: Url = build_public_id(IdKind::Actor, &state.base_url, "", "forge")?;

            let (payload, job_payload) = match event {
                github::PullRequestPayload::Assigned { .. } => {
                    info!("No need to check assignees at the moment. Ignoring");
                    (None, None)
                }
                github::PullRequestPayload::AutoMergeDisabled { .. } => {
                    info!("Not tracking automerge events");
                    (None, None)
                }
                github::PullRequestPayload::AutoMergeEnabled { .. } => {
                    info!("Not tracking automerge events");
                    (None, None)
                }
                github::PullRequestPayload::Closed { shared } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::ConvertedToDraft { shared } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, true)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::ReadyForReview { shared } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::Demilestoned { shared, .. } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::Milestoned { shared, .. } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::Dequeued { .. } => {
                    info!("Not tracking queue events");
                    (None, None)
                }
                github::PullRequestPayload::Edited { shared } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::Opened { shared } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Create(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::Reopened { shared } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
                github::PullRequestPayload::Synchronize { shared, .. } => {
                    let change_request_id: Url = build_public_id(
                        IdKind::ChangeRequest,
                        &state.base_url,
                        &shared.repository.full_name,
                        &shared.number.to_string(),
                    )?;
                    let cr = build_change_request(shared, false, false)?;
                    (
                        Some(forge::Event::Update(forge::ActivityEnvelope {
                            id: change_request_id.clone(),
                            actor: from_actor.clone(),
                            to: vec![to_actor.clone()],
                            cc: vec![],
                            object: forge::ActivityObject::ChangeRequest(cr.clone()),
                        })),
                        Some(forge::Job::GetRecipies {
                            cr_id: change_request_id.clone(),
                            cr,
                        }),
                    )
                }
            };

            if let Some(payload) = payload {
                event!(Level::INFO, cr = ?payload.clone(), "Sending ChangeRequest to Forge for tracking");
                let conn = state.amqp.get().await?;
                let msg = serde_json::to_vec(&payload)?;
                let channel = conn.create_channel().await?;
                channel
                    .basic_publish(
                        &state.inbox,
                        "",
                        BasicPublishOptions::default(),
                        &msg,
                        AMQPProperties::default(),
                    )
                    .await?;
                event!(Level::INFO, "Event Sent");
            }

            if let Some(job_payload) = job_payload {
                event!(Level::INFO, cr = ?job_payload.clone(), "Sending the following Job to workers");
                let conn = state.amqp.get().await?;
                let msg = serde_json::to_vec(&job_payload)?;
                let channel = conn.create_channel().await?;
                channel
                    .basic_publish(
                        &state.job_inbox,
                        "",
                        BasicPublishOptions::default(),
                        &msg,
                        AMQPProperties::default(),
                    )
                    .await?;
                event!(Level::INFO, "Event Sent");
            }
            Ok(())
        }
        GitHubEvent::Issue(_) => Ok(()),
        GitHubEvent::IssueComment(_) => Ok(()),
        GitHubEvent::Status(_) => Ok(()),
        GitHubEvent::Push(push) => {
            event!(Level::DEBUG, push = ?push, "push received");
            Ok(())
        }
        GitHubEvent::Ping(ping) => {
            event!(Level::INFO, ping = ?ping, "ping received");
            Ok(())
        }
    }
}

#[derive(Serialize, Default)]
struct ReturnValue {
    error: Option<String>,
}
