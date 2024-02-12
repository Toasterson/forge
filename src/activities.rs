use serde::{Deserialize, Serialize};
use std::fmt::Display;
use url::Url;

#[derive(Deserialize, Serialize)]
pub enum Scheme {
    HTTP,
    HTTPS,
}

impl From<String> for Scheme {
    fn from(s: String) -> Self {
        match s.as_str() {
            "http" => Scheme::HTTP,
            "https" => Scheme::HTTPS,
            _ => panic!("Invalid scheme"),
        }
    }
}

impl Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scheme::HTTP => write!(f, "http"),
            Scheme::HTTPS => write!(f, "https"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UrlOrEncodedBlob {
    Url(Url),
    Blob(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Event {
    Create(ActivityEnvelope),
    Update(ActivityEnvelope),
    Delete(ActivityEnvelope),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActivityEnvelope {
    pub actor: Url,
    pub to: Vec<Url>,
    pub cc: Vec<Url>,
    pub object: ActivityObject,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ActivityObject {
    ChangeRequest(ChangeRequest),
    JobReport(JobReport),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobReport {
    pub result: JobReportResult,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum JobReportResult {
    Sucess,
    Failure,
    Warning,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeRequest {
    pub changes: Vec<ComponentChange>,
    pub external_ref: ExternalReference,
    pub state: ChangeRequestState,
    pub contributor: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ChangeRequestState {
    Open,
    Closed,
    Applied,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExternalReference {
    GitHub { pull_request: String },
}

impl Display for ExternalReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitHub { pull_request } => write!(f, "github:pr:{}", pull_request),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentChange {
    pub kind: ComponentChangeKind,
    pub component_ref: String,
    pub recipe: serde_json::Value,
    pub recipe_diff: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ComponentChangeKind {
    Added,
    Updated,
    Removed,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum JobObject {
    DownloadSources(DownloadComponentSources),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadComponentSources {
    pub recipe: serde_json::Value,
}
