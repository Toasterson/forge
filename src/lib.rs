use miette::{miette, Diagnostic};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{ParseError, Url};

use component::{Component, PackageMeta, Recipe, RecipeDiff};
use gate::Gate;
use uuid::Uuid;

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

#[derive(Clone, Copy)]
pub enum IdKind {
    Actor,
    ChangeRequest,
}

/// Build a Activitypub ID
///
/// # Errors
///
/// Can fail if we end up with a invalid url but unlikely
pub fn build_public_id(
    kind: IdKind,
    base_url: &Url,
    parent: &str,
    id: &str,
) -> Result<Url, ParseError> {
    match kind {
        IdKind::Actor => base_url.join(&format!("/actors/{id}")),
        IdKind::ChangeRequest => base_url.join(&format!("/objects/changeRequests/{parent}/{id}")),
    }
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
    NotKnown,
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
            Self::Archive => "archive",
            Self::Patch => "patch",
            Self::Script => "script",
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
        write!(
            f,
            "{}:{}:{}:{}",
            kind_str,
            self.component.replace('/', "_"),
            self.name.replace('/', "_"),
            self.hash
        )
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub enum Scheme {
    HTTP,
    HTTPS,
}

impl TryFrom<String> for Scheme {
    type Error = miette::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "http" => Ok(Self::HTTP),
            "https" => Ok(Self::HTTPS),
            _ => Err(miette!("Invalid scheme")),
        }
    }
}

impl Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HTTP => write!(f, "http"),
            Self::HTTPS => write!(f, "https"),
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
    Component { component: Component, gate: String },
    Gate(Gate),
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

#[derive(Debug, Serialize, Deserialize, Clone, Eq, Ord, PartialOrd, PartialEq)]
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
            Self::GitHub { pull_request } => write!(f, "github:pr:{pull_request}"),
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
pub struct PatchFile {
    pub name: String,
    pub content: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JobReport {
    Success(JobReportData),
    Failure {
        /// The Object the job was working on when the error occurred
        object: JobObject,
        /// The error that occurred while processing the object
        error: String,
        /// The Kind of job that ran
        kind: JobKind,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JobObject {
    ChangeRequest { cr_id: Url, gate_id: Uuid },
}

impl Display for JobObject {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChangeRequest { cr_id, gate_id } => {
                write!(f, "ChangeRequest {cr_id} for gate {gate_id}")
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JobKind {
    GetRecipes,
}

impl Display for JobKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GetRecipes => {
                write!(f, "GetRecipes")
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JobReportData {
    GetRecipes {
        gate_id: Uuid,
        change_request_id: String,
        recipes: Vec<(String, Recipe, Option<PackageMeta>, Vec<PatchFile>)>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Job {
    GetRecipes {
        cr_id: Url,
        gate_id: Uuid,
        cr: ChangeRequest,
    },
}
