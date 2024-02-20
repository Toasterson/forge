use component::{Recipe, RecipeDiff};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use url::Url;

#[derive(Deserialize, Serialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum UrlOrEncodedBlob {
    Url(Url),
    Blob(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Event {
    Create(ActivityEnvelope),
    Update(ActivityEnvelope),
    Delete(ActivityEnvelope),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActivityEnvelope {
    pub id: Url,
    pub actor: Url,
    pub to: Vec<Url>,
    pub cc: Vec<Url>,
    pub object: ActivityObject,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ActivityObject {
    ChangeRequest(ChangeRequest),
    JobReport(JobReport),
    Job(JobObject),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JobReport {
    pub result: JobReportResult,
    pub data: JobReportData,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JobReportResult {
    Sucess,
    Failure,
    Warning,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JobReportData {
    ArchiveDownloaded {},
    GetRecipies {
        change_request_id: String,
        recipies: Vec<(String, Recipe)>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChangeRequest {
    pub title: String,
    pub body: String,
    pub changes: Vec<ComponentChange>,
    pub external_ref: ExternalReference,
    pub state: ChangeRequestState,
    pub contributor: String,
    pub labels: Vec<Label>,
    pub milestone: Option<Milestone>,
    pub head: CommitRef,
    pub base: CommitRef,
    pub git_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitRef {
    pub sha: String,
    pub ref_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Label {
    pub name: String,
    pub description: Option<String>,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Milestone {
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ChangeRequestState {
    Open,
    Draft,
    Closed,
    Applied,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ComponentChange {
    pub kind: ComponentChangeKind,
    pub component_ref: String,
    pub recipe: Recipe,
    pub recipe_diff: Option<RecipeDiff>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ComponentChangeKind {
    Added,
    Updated,
    Removed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JobObject {
    DownloadSources(DownloadComponentSources),
    DetectChanges(ChangeRequest),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadComponentSources {
    pub recipe: Recipe,
}
