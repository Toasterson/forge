use clap::{Parser, Subcommand};

use crate::metadata;

#[derive(Debug, Parser)]
pub(crate) struct Args {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    #[clap(name = "metadata")]
    Metadata(metadata::Args),
}

pub(crate) fn run(args: Args) -> miette::Result<()> {
    match args.command {
        Commands::Metadata(args) => metadata::print_component(args),
    }
}
