use ::component::ComponentError;
use directories::ProjectDirs;
use miette::Diagnostic;
use thiserror::Error;

pub mod args;
pub mod build;
mod component;
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

    #[error(transparent)]
    ComponentError(#[from] ComponentError),

    #[error(transparent)]
    StdFsError(#[from] std::io::Error),
}

type Result<T, E = Error> = miette::Result<T, E>;

pub fn get_project_dir() -> Result<ProjectDirs, Error> {
    ProjectDirs::from("org", "OpenIndiana", "pkgdev").ok_or(Error::NoHomeDefined)
}
