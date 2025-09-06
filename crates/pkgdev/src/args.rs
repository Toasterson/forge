use std::fs;
use std::path::{Path, PathBuf};

use crate::build::{run_build, BuildArgs};
use crate::component::open_component_local;
use crate::create::create_component;
use crate::metadata;
use crate::modify::{edit_component, EditArgs};
use crate::repo::RepoManager;
use crate::sources::download_sources;
use clap::{Parser, Subcommand, ValueEnum};
use component::{Component, SourceNode};
use forge_config::Settings;
use gate::Gate;
use miette::{Context, IntoDiagnostic};
use repology::MetadataBuilder;
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
        /// Output file path for generated data (stdout if omitted)
        #[clap(long, short)]
        output: Option<PathBuf>,
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
    Repology,
}

pub async fn run(args: Args) -> miette::Result<()> {
    let gate = if let Some(gate_path) = args.gate {
        tracing::info!(target: "pkgdev::cli", "[pkgdev] Using gate file: {}", gate_path.display());
        let gate = Gate::load(gate_path)?;
        Some(gate)
    } else {
        tracing::info!(target: "pkgdev::cli", "[pkgdev] No gate specified; using current directory as gate root");
        None
    };

    let settings = Settings::open().wrap_err("unable to open app settings")?;

    let wks = if let Some(wks_path) = args.workspace {
        tracing::info!(target: "pkgdev::cli", "[pkgdev] Using workspace override: {}", wks_path.display());
        settings
            .get_workspace_from(wks_path.as_path())
            .wrap_err("unable to open workspace path provided")?
    } else {
        settings
            .get_current_wks()
            .wrap_err("unable to open current workspace path")?
    };

    tracing::info!(target: "pkgdev::cli", "[pkgdev] Subcommand: {:?}", args.command);
    if let Some(p) = &args.repo {
        tracing::info!(target: "pkgdev::cli", "[pkgdev] Repo override path: {}", p.display());
    }
    if let Some(c) = &args.repo_context {
        tracing::info!(target: "pkgdev::cli", "[pkgdev] Repo context override: {}", c);
    }

    match args.command {
        Commands::Repo { cmd } => {
            let mut mgr = RepoManager::load()
                .into_diagnostic()
                .wrap_err("unable to open repo contexts")?;
            match cmd {
                RepoCmd::List => {
                    for r in mgr.list() {
                        // This is intentional stdout output for the CLI list command.
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
                    tracing::info!(target: "pkgdev::repo", "created repo context at {}", resolved_path.display());
                    Ok(())
                }
                RepoCmd::Delete { name } => {
                    mgr.delete(name)
                        .into_diagnostic()
                        .wrap_err("failed to delete repo context")?;
                    tracing::info!(target: "pkgdev::repo", "deleted repo context");
                    Ok(())
                }
                RepoCmd::Select { name } => {
                    mgr.select(name)
                        .into_diagnostic()
                        .wrap_err("failed to select repo context")?;
                    if let Some(cur) = mgr.current() {
                        tracing::info!(target: "pkgdev::repo", "selected {} -> {}", cur.name, cur.path.display());
                    }
                    Ok(())
                }
            }
        }
        Commands::Metadata { args, format } => metadata::print_component(args, format, &gate),
        Commands::Generate { kind, output } => match kind {
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
            GenerateSchemaKind::Repology => generate_repology(&gate, output.as_deref()),
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

fn last_segment<S: AsRef<str>>(s: S) -> String {
    let s = s.as_ref();
    s.rsplit('/').next().unwrap_or(s).to_string()
}

fn find_component_dirs(start: &Path, out: &mut Vec<PathBuf>) -> miette::Result<()> {
    for entry in fs::read_dir(start)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read dir {}", start.display()))?
    {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.is_dir() {
            if path.join("package.kdl").exists() {
                out.push(path);
            } else {
                find_component_dirs(&path, out)?;
            }
        }
    }
    Ok(())
}

fn component_to_repology(c: &Component) -> Option<repology::Metadata> {
    let r = &c.recipe;

    let version_str = match &r.version {
        Some(v) => v.clone(),
        None => {
            tracing::warn!(target: "pkgdev::generate", "skipping {}: missing version", r.name);
            return None;
        }
    };

    // Preserve semverish version strings; do not skip non-strict versions.

    let summary = r.summary.clone().unwrap_or_else(|| r.name.clone());
    let project_name = r
        .project_name
        .clone()
        .unwrap_or_else(|| last_segment(&r.name));
    let source_name = r.project_name.clone().unwrap_or_else(|| r.name.clone());
    let fmri = format!(
        "{}@{}-{}",
        r.name,
        version_str,
        r.revision.clone().unwrap_or_else(|| "0".to_string())
    );

    let mut builder = MetadataBuilder::default();
    builder
        .summary(summary)
        .source_name(source_name)
        .fmri(fmri)
        .project_name(project_name)
        .version(version_str);

    if !r.maintainers.is_empty() {
        builder.maintainers(r.maintainers.clone());
    }

    let mut homepages: Vec<String> = Vec::new();
    if let Some(u) = &r.project_url {
        homepages.push(u.clone());
    }
    builder.homepages(homepages);

    let mut licenses: Vec<String> = Vec::new();
    if let Some(l) = &r.license {
        for part in l.split(',') {
            let t = part.trim();
            if !t.is_empty() {
                licenses.push(t.to_string());
            }
        }
    }
    if !licenses.is_empty() {
        builder.licenses(licenses);
    }

    let mut source_links: Vec<String> = Vec::new();
    for section in &r.sources {
        for src in &section.sources {
            if let SourceNode::Archive(a) = src {
                source_links.push(a.src.clone());
            }
        }
    }
    builder.source_links(source_links);

    let mut categories: Vec<String> = Vec::new();
    if let Some(c) = &r.classification {
        if !c.is_empty() {
            categories.push(c.clone());
        }
    }
    builder.categories(categories);

    match builder.build() {
        Ok(m) => Some(m),
        Err(e) => {
            tracing::warn!(target: "pkgdev::generate", "skipping {}: failed to build repology metadata: {}", r.name, e);
            None
        }
    }
}

fn generate_repology(gate: &Option<Gate>, output: Option<&Path>) -> miette::Result<()> {
    let root = if let Some(g) = gate {
        g.get_gate_path().join("components")
    } else {
        let cwd = std::env::current_dir().into_diagnostic()?;
        let components = cwd.join("components");
        if components.exists() {
            components
        } else {
            cwd
        }
    };

    let mut component_dirs = Vec::new();
    find_component_dirs(&root, &mut component_dirs)?;

    let mut all: Vec<repology::Metadata> = Vec::new();
    for dir in component_dirs {
        match Component::open_local(&dir) {
            Ok(c) => {
                if let Some(m) = component_to_repology(&c) {
                    all.push(m);
                }
            }
            Err(e) => {
                tracing::warn!(target: "pkgdev::generate", "failed to open component at {}: {}", dir.display(), e);
            }
        }
    }

    let json = serde_json::to_string_pretty(&all).into_diagnostic()?;
    if let Some(path) = output {
        fs::write(path, json)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write output file {}", path.display()))?;
    } else {
        println!("{}", json);
    }
    Ok(())
}
