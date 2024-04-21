use std::fmt::{Display, Formatter};
use std::str::FromStr;
use miette::Diagnostic;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{ParseError, Url};

use component::{Component, Recipe, RecipeDiff};
use gate::Gate;

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<OpenIdConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<OpenIdConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenIdConfig {
    pub client_id: String,
}

pub enum IdKind {
    Actor,
    ChangeRequest,
}
pub fn build_public_id(kind: IdKind, base_url: &Url, parent: &str, id: &str) -> Result<Url, ParseError> {
    match kind {
        IdKind::Actor => format!("{}/actors/{}", base_url, id),
        IdKind::ChangeRequest => format!("{}/objects/changeRequests/{}/{}", base_url, parent, id),
    }.parse()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum ComponentFileKind {
    Patch,
    Script,
    Archive,
}

#[derive(Error, Debug, Diagnostic)]
pub enum FileKindError {
    #[error("file kind not known use patch, archive or script")]
    NotKnown
}
impl FromStr for ComponentFileKind {
    type Err = FileKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "patch" => Ok(Self::Patch),
            "archive" => Ok(Self::Archive),
            "script" => Ok(Self::Script),
            _ => Err(Self::Err::NotKnown),
        }
    }
}
impl Display for ComponentFileKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ComponentFileKind::Archive => "archive",
            ComponentFileKind::Patch => "patch",
            ComponentFileKind::Script => "script",
        };
        write!(f, "{name}")
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ComponentFile {
    pub kind: ComponentFileKind,
    pub component: String,
    pub name: String,
    pub hash: String,
}

impl Display for ComponentFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let kind_str = match self.kind {
            ComponentFileKind::Patch => "patch",
            ComponentFileKind::Script => "script",
            ComponentFileKind::Archive => "archive",
        };
        write!(f, "{}:{}:{}:{}", kind_str, self.component.replace("/", "_"), self.name.replace("/", "_"), self.hash)
    }
}

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
    Component{
        component: Component,
        gate: String,
    },
    Gate(Gate)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JobReport {
    pub related_object: Url,
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
    GetRecipies {
        change_request_id: String,
        recipies: Vec<(String, Recipe)>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChangeRequest {
    pub id: String,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
    Processing,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Job {
    GetRecipies{
        cr_id: Url,
        cr: ChangeRequest
    },
}
