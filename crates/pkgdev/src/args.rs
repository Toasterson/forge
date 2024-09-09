use std::path::PathBuf;

use crate::build::{run_build, BuildArgs};
use crate::component::open_component_local;
use crate::create::create_component;
use crate::forge::{handle_forge_interaction, ForgeArgs};
use crate::metadata;
use crate::modify::{edit_component, EditArgs};
use crate::sources::download_sources;
use clap::{Parser, Subcommand, ValueEnum};
use config::Settings;
use gate::Gate;
use miette::IntoDiagnostic;
use strum::Display;

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(long, global = true)]
    /// Path to the gate kdl file adding gate wide settings to this all components
    pub gate: Option<PathBuf>,

    /// Allows one to change the workspace for this operation only. Intended for the CI usecase so that
    /// multiple jobs can be run simultaneously
    #[arg(long, short)]
    workspace: Option<PathBuf>,

    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[clap(name = "download")]
    Download {
        #[clap(short, long, default_value = ".")]
        component: PathBuf,
    },
    #[clap(name = "metadata")]
    Metadata {
        #[clap(flatten)]
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
    #[clap(name = "forge")]
    Forge {
        #[clap(subcommand)]
        args: ForgeArgs,
    },
    #[clap(name = "build")]
    Build {
        #[arg(short, long, default_value = ".")]
        component: PathBuf,

        #[command(flatten)]
        args: BuildArgs,
    }
}

#[derive(Debug, Parser, Clone)]
pub struct ComponentArgs {
    #[clap(short, long, default_value = ".")]
    pub component: PathBuf,
}

#[derive(Debug, Default, Display, Clone, ValueEnum)]
#[strum(serialize_all = "kebab-case")]
pub enum GenerateSchemaKind {
    #[default]
    ComponentRecipe,
    ForgeIntegrationManifest,
}

pub async fn run(args: Args) -> miette::Result<()> {
    let gate = if let Some(gate_path) = args.gate {
        let gate = Gate::new(gate_path)?;
        Some(gate)
    } else {
        None
    };

    let settings = Settings::open().into_diagnostic()?;

    let wks = if let Some(wks_path) = args.workspace {
        settings.get_workspace_from(wks_path.as_path()).into_diagnostic()?
    } else {
        settings.get_current_wks().into_diagnostic()?
    };

    match args.command {
        Commands::Metadata { args, format } => metadata::print_component(args, format),
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
        } => {
            let component = open_component_local(&component, &gate)?;
            download_sources(&component, &wks, true).await
        },
        Commands::Create { fmri, args } => create_component(args, fmri),
        Commands::Edit { component, args } => edit_component(component, gate, args),
        Commands::Forge { args } => Ok(handle_forge_interaction(&args).await?),
        Commands::Build { component, args } => {
            let component = open_component_local(component, &gate)?;
            run_build(&component, &gate, &wks, &settings, &args).await
        }
    }
}
