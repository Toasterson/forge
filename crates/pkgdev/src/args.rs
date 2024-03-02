use std::path::PathBuf;

use crate::create::create_component;
use clap::{Parser, Subcommand, ValueEnum};
use miette::IntoDiagnostic;
use strum::Display;

use crate::metadata;
use crate::modify::{edit_component, EditArgs};
use crate::sources::download_sources;

#[derive(Debug, Parser)]
pub(crate) struct Args {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    #[clap(name = "download")]
    Download {
        #[clap(short, long, default_value = ".")]
        component: PathBuf,
        #[clap(default_value = ".")]
        target_dir: PathBuf,
    },
    #[clap(name = "metadata")]
    Metadata{
        args: ComponentArgs,
        #[clap(default_value_t = metadata::MetadataFormat::default())]
        format: metadata::MetadataFormat,
    },
    #[clap(name = "generate")]
    Generate {
        #[clap(default_value_t = GenerateSchemaKind::default())]
        kind: GenerateSchemaKind,
    },
    #[clap(name = "create")]
    Create {
        fmri: String,
        #[clap(flatten)]
        args: ComponentArgs,
    },
    #[clap(name = "edit")]
    Edit {
        #[clap(short, long, default_value = ".")]
        component: PathBuf,
        #[clap(subcommand)]
        args: EditArgs,
    },
}

#[derive(Debug, Parser, Clone)]
pub(crate) struct ComponentArgs {
    #[clap(short, long, default_value = ".")]
    pub component: PathBuf,
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
        Commands::Metadata{args, format } => metadata::print_component(args, format),
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
        Commands::Download {
            component,
            target_dir,
        } => download_sources(component, target_dir),
        Commands::Create { fmri, args } => create_component(args, fmri),
        Commands::Edit { component, args } => edit_component(component, args),
    }
}
