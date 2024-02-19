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
use tracing::{debug, error, event, info, instrument, Level};

use forge::{ChangeRequest, ChangeRequestState, CommitRef, Label, Milestone, Scheme};
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

#[instrument]
async fn handle_webhook(State(state): State<AppState>, req: GitHubWebhookRequest) -> Result<()> {
    debug!("Received Webhook: {}", req.get_kind());
    match req.get_event()? {
        GitHubEvent::PullRequest(event) => {
            event!(Level::INFO, pr = ?event.clone(), "Received pull request event");
            let from_actor = format!("{}/actors/github", &state.base_url).parse()?;
            let to_actor = format!("{}/actors/forge", &state.base_url).parse()?;

            let payload = match event {
                github::PullRequestPayload::Assigned { .. } => {
                    info!("No need to check asignees at the moment. Ignoring");
                    None
                }
                github::PullRequestPayload::AutoMergeDisabled { .. } => {
                    info!("Not tracking automerge events");
                    None
                }
                github::PullRequestPayload::AutoMergeEnabled { .. } => {
                    info!("Not tracking automerge events");
                    None
                }
                github::PullRequestPayload::Closed { shared } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: false,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: ChangeRequestState::Closed,
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::ConvertedToDraft { shared } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: ChangeRequestState::Draft,
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::ReadyForReview { shared } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: ChangeRequestState::Open,
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::Demilestoned { shared, .. } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: match shared.pull_request.state {
                                github::PullRequestState::Open => ChangeRequestState::Open,
                                github::PullRequestState::Closed => ChangeRequestState::Closed,
                            },
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::Milestoned { shared, .. } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: match shared.pull_request.state {
                                github::PullRequestState::Open => ChangeRequestState::Open,
                                github::PullRequestState::Closed => ChangeRequestState::Closed,
                            },
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::Dequeued { .. } => {
                    info!("Not tracking queue events");
                    None
                }
                github::PullRequestPayload::Edited { shared } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: match shared.pull_request.state {
                                github::PullRequestState::Open => ChangeRequestState::Open,
                                github::PullRequestState::Closed => ChangeRequestState::Closed,
                            },
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::Opened { shared } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Create(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: ChangeRequestState::Open,
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::Reopened { shared } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: ChangeRequestState::Open,
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
                github::PullRequestPayload::Synchronize { shared, .. } => {
                    let change_request_id = format!(
                        "{}/objects/changeRequests/{}",
                        &state.base_url, shared.number
                    )
                    .parse()?;
                    Some(forge::Event::Update(forge::ActivityEnvelope {
                        id: change_request_id,
                        actor: from_actor,
                        to: vec![to_actor],
                        cc: vec![],
                        object: forge::ActivityObject::ChangeRequest(ChangeRequest {
                            title: shared.pull_request.title,
                            body: shared.pull_request.body.unwrap_or(String::new()),
                            changes: vec![],
                            dirty: true,
                            external_ref: forge::ExternalReference::GitHub {
                                pull_request: shared.pull_request.url,
                            },
                            state: match shared.pull_request.state {
                                github::PullRequestState::Open => ChangeRequestState::Open,
                                github::PullRequestState::Closed => ChangeRequestState::Closed,
                            },
                            contributor: format!("{}@github.com", shared.sender.login),
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
                            head: CommitRef {
                                sha: shared.pull_request.head.sha,
                                ref_name: shared.pull_request.head.ref_name,
                            },
                            base: CommitRef {
                                sha: shared.pull_request.base.sha,
                                ref_name: shared.pull_request.base.ref_name,
                            },
                        }),
                    }))
                }
            };

            if let Some(payload) = payload {
                event!(Level::INFO, cr = ?payload.clone(), "Built change request event");
                let conn = state.amqp.get().await?;
                let msg = serde_json::to_vec(&payload)?;
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
            }
            Ok(())
        }
        GitHubEvent::Issue(_) => Ok(()),
        GitHubEvent::IssueComment(_) => Ok(()),
        GitHubEvent::Status(_) => Ok(()),
        GitHubEvent::Push(_) => Ok(()),
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
