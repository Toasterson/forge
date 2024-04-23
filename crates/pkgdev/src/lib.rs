use directories::ProjectDirs;
use miette::Diagnostic;
use thiserror::Error;

pub mod args;
pub mod create;
pub mod forge;
pub mod metadata;
pub mod modify;
pub mod openid;
pub mod sources;
#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error("no $HOME directory defined")]
    NoHomeDefined,
}
pub fn get_project_dir() -> Result<ProjectDirs, Error> {
    ProjectDirs::from("org", "OpenIndiana", "pkgdev").ok_or(Error::NoHomeDefined)
}
