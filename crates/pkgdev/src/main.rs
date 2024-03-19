use clap::Parser;
use directories::ProjectDirs;
use miette::Diagnostic;
use thiserror::Error;

use crate::args::run;

mod args;
mod create;
mod forge;
mod metadata;
mod modify;
mod sources;

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error("no $HOME directory defined")]
    NoHomeDefined,
}
pub fn get_project_dir() -> Result<ProjectDirs, Error> {
    ProjectDirs::from("org", "OpenIndiana", "pkgdev").ok_or(Error::NoHomeDefined)
}

fn main() -> miette::Result<()> {
    let args = args::Args::parse();
    run(args)?;
    Ok(())
}
