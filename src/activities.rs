use std::fmt::Display;

use serde::{Deserialize, Serialize};
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
    Push(Push),
    MergeRequest(MergeRequest),
    PackageRepository(PackageRepository),
    Package(Package),
    SoftwareComponent(SoftwareComponent),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergeRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub head: CommitRef,
    pub base: CommitRef,
    pub repository: String,
    pub origin_url: Option<String>,
    pub action: String,
    /// optional the url with the patch to apply to the repository before building
    pub patch: Option<Url>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitRef {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SoftwareComponent {
    pub name: String,
    pub recipe_files: Vec<Url>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageRepository {
    pub name: String,
    pub publishers: Vec<String>,
    pub url: Url,
    pub public_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Push {
    /// Sha hash before the commit
    pub before: String,
    /// Sha hash after the commit
    pub after: String,
    /// ref of this commit that should be built
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// repository this job is supposed to build
    pub repository: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Commit {
    /// An array of files added in the commit.
    pub added: Vec<String>,
    /// Metaproperties for Git author/committer information.
    pub author: Actor,
    /// Metaproperties for Git author/committer information.
    pub committer: Actor,
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
    pub url: Url,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Actor {
    /// The name of the actor.
    pub name: String,
    /// The email of the actor.
    pub email: String,
    /// The username of the actor.
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    /// Optional URL with a patch to apply to the repository before building.
    pub patch: Option<Url>,
    /// The reference to pass to git to pull the correct basis for the build.
    pub ref_name: String,
    /// Optional a ref that can be used to construct the list of changed files in the build.
    pub base_ref: Option<String>,
    /// repository this job is related to. Must be the Forge Known repository and not a fork
    pub repository: String,
    /// the reference to pass to git with the build config if the source branch is not trusted (for example in pull requests)
    pub conf_ref: Option<String>,
    /// any tags given to the Job
    pub tags: Option<Vec<String>>,
    /// optional the type of the job that the system needs to run
    pub job_type: Option<KnownJobs>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum KnownJobs {
    CheckForChangedComponents,
}
