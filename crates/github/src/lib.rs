use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{
        rejection::{BytesRejection, JsonRejection, TypedHeaderRejection},
        FromRequest, TypedHeader,
    },
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tracing::trace;

use headers::Signature;

use crate::headers::Event;

pub mod headers {
    use axum::{headers::Header, http::HeaderName};
    use strum::{Display, EnumString};

    pub static SIGNATURE: HeaderName = HeaderName::from_static("x-hub-signature-256");

    #[derive(Debug)]
    pub struct Signature(String);

    impl Header for Signature {
        fn name() -> &'static HeaderName {
            &SIGNATURE
        }

        fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
        where
            Self: Sized,
            I: Iterator<Item = &'i axum::http::HeaderValue>,
        {
            let value = values.next().ok_or_else(axum::headers::Error::invalid)?;
            Ok(Signature(
                value
                    .to_str()
                    .map_err(|_| axum::headers::Error::invalid())?
                    .to_string(),
            ))
        }

        fn encode<E: Extend<axum::http::HeaderValue>>(&self, _values: &mut E) {
            todo!()
        }
    }

    pub static HOOK_ID: HeaderName = HeaderName::from_static("x-github-hook-id");

    pub static EVENT: HeaderName = HeaderName::from_static("x-github-event");

    #[derive(Debug, Clone, Display, EnumString)]
    pub enum Event {
        Ping,
        Push,
        PullRequest,
    }

    impl Header for Event {
        fn name() -> &'static HeaderName {
            &EVENT
        }

        fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
        where
            Self: Sized,
            I: Iterator<Item = &'i axum::http::HeaderValue>,
        {
            let value = values.next().ok_or_else(axum::headers::Error::invalid)?;
            match value
                .to_str()
                .map_err(|_| axum::headers::Error::invalid())?
            {
                "push" => Ok(Self::Push),
                "ping" => Ok(Self::Ping),
                "pull_request" => Ok(Self::PullRequest),
                _ => Err(axum::headers::Error::invalid()),
            }
        }

        fn encode<E: Extend<axum::http::HeaderValue>>(&self, _values: &mut E) {
            todo!()
        }
    }

    pub static DELIVERY: HeaderName = HeaderName::from_static("x-github-delivery");

    pub static TARGET_TYPE: HeaderName =
        HeaderName::from_static("x-github-hook-installation-target-type");

    pub static TARGET_ID: HeaderName =
        HeaderName::from_static("x-github-hook-installation-target-id");
}

#[derive(FromRequest, Debug)]
#[from_request(rejection(GitHubError))]
pub struct GitHubWebhookRequest {
    #[allow(dead_code)]
    #[from_request(via(TypedHeader))]
    signature: Signature,
    #[from_request(via(TypedHeader))]
    event_kind: headers::Event,
    body: Bytes,
}

impl std::fmt::Display for GitHubWebhookRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Event Request: {}", self.event_kind)
    }
}

impl GitHubWebhookRequest {
    pub fn get_kind(&self) -> Event {
        self.event_kind.clone()
    }

    pub fn get_event(&self) -> Result<GitHubEvent, GitHubError> {
        trace!("Parsing github event {}", self.event_kind);
        match self.event_kind {
            Event::Ping => Ok(GitHubEvent::Ping(serde_json::from_slice(&self.body)?)),
            Event::Push => Ok(GitHubEvent::Push(serde_json::from_slice(&self.body)?)),
            Event::PullRequest => Ok(GitHubEvent::PullRequest(serde_json::from_slice(
                &self.body,
            )?)),
        }
    }
}

pub enum GitHubEvent {
    PullRequest(PullRequestPayload),
    Issue(Issue),
    IssueComment(IssueComment),
    Status(Status),
    Push(Push),
    Ping(Ping),
}

#[derive(Error, Debug)]
pub enum GitHubError {
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error("{0}")]
    Value(String),

    #[error("Json {status}: {message}")]
    JsonRejection { status: StatusCode, message: String },

    #[error("Could not extract header: {name}: {reason}")]
    TypedHeaderRejection { name: String, reason: String },

    #[error("invalid bytes in body: {message}")]
    BytesRejection { status: StatusCode, message: String },
}

// We implement `From<JsonRejection> for ApiError`
impl From<JsonRejection> for GitHubError {
    fn from(rejection: JsonRejection) -> Self {
        Self::JsonRejection {
            status: rejection.status(),
            message: rejection.body_text(),
        }
    }
}

impl From<TypedHeaderRejection> for GitHubError {
    fn from(value: TypedHeaderRejection) -> Self {
        Self::TypedHeaderRejection {
            name: value.name().to_string(),
            reason: match value.reason() {
                axum::extract::rejection::TypedHeaderRejectionReason::Missing => {
                    String::from("missing header")
                }
                axum::extract::rejection::TypedHeaderRejectionReason::Error(err) => err.to_string(),
                _ => todo!(),
            },
        }
    }
}

impl From<BytesRejection> for GitHubError {
    fn from(value: BytesRejection) -> Self {
        Self::BytesRejection {
            status: value.status(),
            message: value.body_text(),
        }
    }
}

impl IntoResponse for GitHubError {
    fn into_response(self) -> axum::response::Response {
        trace!("We have an error: {:?}", self);
        match self {
            Self::JsonRejection { status, message } => (status, message),
            Self::TypedHeaderRejection { .. } => (StatusCode::BAD_REQUEST, String::new()),
            Self::BytesRejection { status, message } => (status, message),
            err => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        .into_response()
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PullRequestPayload {
    Assigned {
        assignee: Option<User>,
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    AutoMergeDisabled {
        reason: String,
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    AutoMergeEnabled {
        reason: String,
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Closed {
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    ConvertedToDraft {
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    ReadyForReview {
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Demilestoned {
        milestone: Option<Milestone>,
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Milestoned {
        milestone: Option<Milestone>,
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Dequeued {
        reason: String,
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Edited {
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Opened {
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Reopened {
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
    Synchronize {
        after: String,
        before: String,
        #[serde(flatten)]
        shared: PullRequestPayloadSharedFields,
    },
}

#[derive(Debug, Deserialize, Clone)]
pub struct Milestone {
    pub id: i32,
    pub url: String,
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PullRequestPayloadSharedFields {
    /// The pull request number.
    pub number: i32,
    /// The actual pull_request struct
    pub pull_request: PullRequestObject,
    /// The repository on GitHub where the event occurred. Webhook payloads contain the repository property when the event occurs from activity in a repository.
    pub repository: Repository,
    /// A GitHub organization. Webhook payloads contain the organization property when the webhook is configured for an organization, or when the event occurs from activity in a repository owned by an organization.
    pub organization: Option<HashMap<String, Value>>,
    /// The GitHub App installation. Webhook payloads contain the installation property when the event is configured for and sent to a GitHub App. For more information, see "Using webhooks with GitHub Apps."
    pub installation: Option<HashMap<String, Value>>,
    /// An enterprise on GitHub. Webhook payloads contain the enterprise property when the webhook is configured on an enterprise account or an organization that's part of an enterprise account. For more information, see "About enterprise accounts."
    pub enterprise: Option<HashMap<String, Value>>,
    /// The GitHub user that triggered the event. This property is included in every webhook payload.
    pub sender: User,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PullRequestObject {
    pub url: String,
    pub id: i32,
    pub patch_url: String,
    pub node_id: String,
    /// Number uniquely identifying the pull request within its repository.
    pub number: i32,
    /// State of this Pull Request. Either open or closed.
    pub state: PullRequestState,
    pub locked: bool,
    /// The title of the pull request.
    pub title: String,
    /// A GitHub user.
    pub user: HashMap<String, serde_json::Value>,
    /// body of the message
    pub body: Option<String>,
    pub head: CommitRef,
    pub base: CommitRef,
    pub draft: bool,
    pub merged: bool,
    pub mergable: Option<bool>,
    pub author_association: AuthorAssociation,
    pub labels: Vec<Label>,
    pub milestone: Option<Milestone>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Label {
    pub id: i32,
    pub node_id: String,
    pub url: String,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub default: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorAssociation {
    Collaborator,
    Contributor,
    FirstTimer,
    FirstTimeContributor,
    Mannequin,
    Member,
    None,
    Owner,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CommitRef {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    pub name: Option<String>,
    pub email: Option<String>,
    pub login: Option<String>,
    pub id: i32,
    pub url: String,
    #[serde(rename = "type")]
    pub kind: UserKind,
}

#[derive(Debug, Deserialize, Clone)]
pub enum UserKind {
    Bot,
    User,
    Organisation,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum PullRequestState {
    Open,
    Closed,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Issue {}

#[derive(Debug, Deserialize, Clone)]
pub struct IssueComment {}

#[derive(Debug, Deserialize, Clone)]
pub struct Status {}

#[derive(Debug, Deserialize, Clone)]
pub struct Push {
    /// The SHA of the most recent commit on ref after the push.
    pub after: String,
    /// Base ref that was pushed
    pub base_ref: Option<String>,
    /// The SHA of the most recent commit on ref before the push.
    pub before: String,
    /// An array of commit objects describing the pushed commits. (Pushed commits are all commits that are included in the compare between the before commit and the after commit.) The array includes a maximum of 20 commits. If necessary, you can use the Commits API to fetch additional commits. This limit is applied to timeline events only and isn't applied to webhook deliveries.
    pub commits: Vec<Commit>,
    /// Head commit (not documented but discovered in the Hookdeck example)
    pub head_commit: Option<Commit>,
    /// URL that shows the changes in this ref update, from the before commit to the after commit. For a newly created ref that is directly based on the default branch, this is the comparison between the head of the default branch and the after commit. Otherwise, this shows all commits until the after commit.
    pub compare: url::Url,
    /// Whether this push created the ref.
    pub created: bool,
    /// Whether this push deleted the ref.
    pub deleted: bool,
    /// An enterprise on GitHub. Webhook payloads contain the enterprise property when the webhook is configured on an enterprise account or an organization that's part of an enterprise account. For more information, see "About enterprise accounts."
    pub enterprise: Option<HashMap<String, serde_json::Value>>,
    /// Whether this push was a force push of the ref.
    pub forced: bool,
    /// The GitHub App installation. Webhook payloads contain the installation property when the event is configured for and sent to a GitHub App. For more information, see "Using webhooks with GitHub Apps."
    pub installation: Option<serde_json::Value>,
    /// A GitHub organization. Webhook payloads contain the organization property when the webhook is configured for an organization, or when the event occurs from activity in a repository owned by an organization.
    pub organization: Option<serde_json::Value>,
    /// Metaproperties for Git author/committer information.
    pub pusher: Author,
    /// The full git ref that was pushed. Example: refs/heads/main or refs/tags/v3.14.1.
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// A git repository
    pub repository: Repository,
    /// The GitHub user that triggered the event. This property is included in every webhook payload.
    pub sender: serde_json::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Commit {
    /// An array of files added in the commit.
    pub added: Vec<String>,
    /// Metaproperties for Git author/committer information.
    pub author: Author,
    /// Metaproperties for Git author/committer information.
    pub committer: Author,
    /// Whether this commit is distinct from any that have been pushed before.
    pub distinct: Option<bool>,
    pub id: String,
    /// The commit message.
    pub message: String,
    /// An array of files modified by the commit.
    pub modified: Vec<String>,
    /// An array of files removed in the commit.
    pub removed: Vec<String>,
    /// The ISO 8601 timestamp of the commit.
    pub timestamp: String,
    pub tree_id: String,
    /// URL that points to the commit API resource.
    pub url: url::Url,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Author {
    pub date: Option<String>,
    pub email: Option<String>,
    /// The git author's name.
    pub name: String,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Repository {
    pub id: i32,
    pub node_id: String,
    pub full_name: String,
    pub git_url: String,
    pub ssh_url: String,
    pub url: String,
    pub private: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Ping {
    pub hook: HashMap<String, serde_json::Value>,
    pub hook_id: i32,
    pub organization: Option<HashMap<String, serde_json::Value>>,
    pub repository: HashMap<String, serde_json::Value>,
    pub sender: HashMap<String, serde_json::Value>,
}
