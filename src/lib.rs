use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Serialize, Deserialize)]
pub enum Event {
    Create(Object),
}

#[derive(Serialize, Deserialize)]
pub struct ActivityEnvelope {
    pub to: Vec<Url>,
    pub cc: Vec<Url>,
    pub object: Object,
}

#[derive(Serialize, Deserialize)]
pub enum Object {
    Job(Job),
}

#[derive(Serialize, Deserialize)]
pub struct Job {
    /// Sha hash before the commit
    pub before: String,
    /// Sha hash after the commit
    pub after: String,
    /// ref of this commit that should be built
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// repository this job is supposed to build
    pub repository: String,
    /// branch with the build config if the source branch is not trusted (for example in pull requests)
    pub conf_ref: Option<String>,
    /// any tags given to the Job
    pub tags: Option<Vec<String>>,
}
