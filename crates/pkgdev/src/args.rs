use std::path::PathBuf;

use crate::build::{run_build, BuildArgs};
use crate::component::open_component_local;
use crate::create::create_component;
use crate::metadata;
use crate::modify::{edit_component, EditArgs};
use crate::repo::RepoManager;
use crate::sources::download_sources;
use clap::{Parser, Subcommand, ValueEnum};
use forge_config::Settings;
use gate::Gate;
use miette::{Context, IntoDiagnostic};
use strum::Display;

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(long, global = true)]
    /// Path to the gate .kdl file. If omitted, the current working directory is treated as the gate root
    /// for resolving components (i.e., <cwd>/components/<component>). Provide --gate only when the gate
    /// is not the current directory.
    pub gate: Option<PathBuf>,

    /// Allows one to change the workspace for this operation only. Intended for the CI usecase so that
    /// multiple jobs can be run simultaneously
    #[arg(long, short)]
    pub workspace: Option<PathBuf>,

    /// Override the repository path to publish to for this command invocation.
    #[arg(long, global = true)]
    pub repo: Option<PathBuf>,

    /// Select a named repository context for this command invocation.
    #[arg(long = "repo-context", global = true)]
    pub repo_context: Option<String>,

    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[clap(name = "repo")]
    Repo {
        #[clap(subcommand)]
        cmd: RepoCmd,
    },
    #[clap(name = "download")]
    Download {
        /// Component folder path relative to the gate's components directory (e.g., `ffmpeg` or `web/firefox`).
        /// If omitted, current directory is used. Absolute paths are accepted.
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
        /// Component folder path relative to the gate's components directory (e.g., `ffmpeg` or `web/firefox`).
        /// If omitted, current directory is used. Absolute paths are accepted.
        #[clap(short, long, default_value = ".")]
        component: PathBuf,
        #[clap(subcommand)]
        args: EditArgs,
    },
    // #[clap(name = "forge")]
    // Forge {
    //     #[clap(subcommand)]
    //     args: ForgeArgs,
    // },
    #[clap(name = "build")]
    Build {
        /// Component folder path relative to the gate's components directory (e.g., `ffmpeg` or `web/firefox`).
        /// If omitted, current directory is used. Absolute paths are accepted.
        #[arg(short, long, default_value = ".")]
        component: PathBuf,

        #[command(flatten)]
        args: BuildArgs,
    },
}

#[derive(Debug, Parser, Clone)]
pub struct ComponentArgs {
    /// Component folder path relative to the gate's components directory (e.g., `ffmpeg` or `web/firefox`).
    /// If omitted, current directory is used. Absolute paths are accepted.
    #[clap(short, long, default_value = ".")]
    pub component: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum RepoCmd {
    #[clap(name = "list")]
    List,
    #[clap(name = "create")]
    Create { name: String, path: Option<PathBuf> },
    #[clap(name = "delete")]
    Delete { name: String },
    #[clap(name = "select")]
    Select { name: String },
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
        let gate = Gate::load(gate_path)?;
        Some(gate)
    } else {
        None
    };

    let settings = Settings::open().wrap_err("unable to open app settings")?;

    let wks = if let Some(wks_path) = args.workspace {
        settings
            .get_workspace_from(wks_path.as_path())
            .wrap_err("unable to open workspace path provided")?
    } else {
        settings
            .get_current_wks()
            .wrap_err("unable to open current workspace path")?
    };

    match args.command {
        Commands::Repo { cmd } => {
            let mut mgr = RepoManager::load()
                .into_diagnostic()
                .wrap_err("unable to open repo contexts")?;
            match cmd {
                RepoCmd::List => {
                    for r in mgr.list() {
                        println!("{}\t{}", r.name, r.path.display());
                    }
                    Ok(())
                }
                RepoCmd::Create { name, path } => {
                    let resolved_path = match path {
                        Some(p) => p,
                        None => {
                            let base = Settings::get_or_create_appdata_dir()
                                .into_diagnostic()
                                .wrap_err("failed to resolve APPDATA directory")?;
                            base.join(&name)
                        }
                    };
                    mgr.create(&name, &resolved_path)
                        .into_diagnostic()
                        .wrap_err("failed to create repo context")?;
                    println!("created repo context at {}", resolved_path.display());
                    Ok(())
                }
                RepoCmd::Delete { name } => {
                    mgr.delete(name)
                        .into_diagnostic()
                        .wrap_err("failed to delete repo context")?;
                    println!("deleted repo context");
                    Ok(())
                }
                RepoCmd::Select { name } => {
                    mgr.select(name)
                        .into_diagnostic()
                        .wrap_err("failed to select repo context")?;
                    if let Some(cur) = mgr.current() {
                        println!("selected {} -> {}", cur.name, cur.path.display());
                    }
                    Ok(())
                }
            }
        }
        Commands::Metadata { args, format } => metadata::print_component(args, format, &gate),
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
        Commands::Download { component } => {
            let component = open_component_local(&component, &gate)?;
            download_sources(&component, &wks, true)
                .await
                .wrap_err("download failed")
        }
        Commands::Create { fmri, args } => create_component(args, fmri),
        Commands::Edit { component, args } => edit_component(component, gate, args),
        //Commands::Forge { args } => Ok(handle_forge_interaction(&args).await?),
        Commands::Build {
            component,
            args: build_args,
        } => {
            let component =
                open_component_local(component, &gate).wrap_err("cannot open component")?;
            let repo_mgr = RepoManager::load()
                .into_diagnostic()
                .wrap_err("unable to open repo contexts")?;
            run_build(
                &component,
                &gate,
                &wks,
                &settings,
                &build_args,
                &repo_mgr,
                args.repo.clone(),
                args.repo_context.clone(),
            )
            .await
            .wrap_err("build failed")
        }
    }
}
