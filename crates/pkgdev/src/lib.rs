use ::component::ComponentError;
use directories::ProjectDirs;
use miette::Diagnostic;
use thiserror::Error;

pub mod api;
pub mod args;
pub mod auth;
pub mod build;
mod component;
pub mod component_client;
pub mod create;
pub mod gate_client;
pub mod metadata;
pub mod modify;
pub mod repo;
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
