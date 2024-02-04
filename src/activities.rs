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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeRequest {
    pub changes: Vec<ComponentChange>,
    pub external_ref: ExternalReference,
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
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ComponentChangeKind {
    Added,
    Updated,
    Removed,
}
