use clap::{Parser, Subcommand, ValueEnum};
use miette::IntoDiagnostic;
use strum::Display;

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
    #[clap(name = "generate")]
    Generate {
        #[clap(default_value_t = GenerateSchemaKind::default())]
        kind: GenerateSchemaKind,
    },
}

#[derive(Debug, Default, Display, Clone, ValueEnum)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum GenerateSchemaKind {
    #[default]
    ComponentRecipe,
    ForgeIntegrationManifest,
}

pub(crate) fn run(args: Args) -> miette::Result<()> {
    match args.command {
        Commands::Metadata(args) => metadata::print_component(args),
        Commands::Generate { kind } => match kind {
            GenerateSchemaKind::ComponentRecipe => {
                let schema = component::get_schema();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&schema).into_diagnostic()?
                );
                Ok(())
            }
            GenerateSchemaKind::ForgeIntegrationManifest => {
                let schema = integration::get_schema();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&schema).into_diagnostic()?
                );
                Ok(())
            }
        },
    }
}
