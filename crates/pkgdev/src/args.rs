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

use crate::auth::{
    self, default_auth_state_path, server_url_from_host, ActorKind, AuthClient, AuthState,
    LoginEntry,
};

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
    #[clap(name = "auth")]
    Auth {
        #[clap(subcommand)]
        cmd: AuthCmd,
    },
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
pub enum AuthCmd {
    /// Register a new actor with their SSH public key on a forge
    Register {
        /// Forge hostname (or hostname:port) to talk to (gRPC)
        #[arg(long)]
        host: String,
        /// Actor identifier (e.g., email for users)
        #[arg(long)]
        actor_id: String,
        /// Actor kind
        #[arg(long, value_enum, default_value_t = ActorKind::User)]
        kind: ActorKind,
        /// Path to the SSH public key file (OpenSSH format)
        #[arg(long)]
        public_key: PathBuf,
        /// Optional algorithm hint (ed25519, rsa-ssh, ecdsa-p256, ...)
        #[arg(long)]
        algorithm: Option<String>,
    },
    /// Confirm a pending registration with an envelope file
    Confirm {
        #[arg(long)]
        host: String,
        #[arg(long)]
        actor_id: String,
        #[arg(long, value_enum, default_value_t = ActorKind::User)]
        kind: ActorKind,
        /// Path to the envelope JSON you received via email
        #[arg(long)]
        envelope: PathBuf,
    },
    /// Mark yourself as logged in for a given forge/actor locally
    Login {
        #[arg(long)]
        host: String,
        #[arg(long)]
        actor_id: String,
        #[arg(long, value_enum, default_value_t = ActorKind::User)]
        kind: ActorKind,
    },
    /// List current logins. If --host is provided, only show that forge.
    List {
        #[arg(long)]
        host: Option<String>,
    },
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
        Commands::Auth { cmd } => match cmd {
            AuthCmd::Register {
                host,
                actor_id,
                kind,
                public_key,
                algorithm,
            } => {
                let url = server_url_from_host(&host);
                let client = AuthClient::connect(url)
                    .await
                    .wrap_err("failed to connect to forge host")?;
                client
                    .register_actor(actor_id, kind, &public_key, algorithm)
                    .await
                    .wrap_err("registration RPC failed")?;
                println!("registration submitted on {}", host);
                Ok(())
            }
            AuthCmd::Confirm {
                host,
                actor_id,
                kind,
                envelope,
            } => {
                let url = server_url_from_host(&host);
                let client = AuthClient::connect(url)
                    .await
                    .wrap_err("failed to connect to forge host")?;
                client
                    .confirm_registration(actor_id, kind, &envelope)
                    .await
                    .wrap_err("confirmation RPC failed")?;
                println!("registration confirmation sent on {}", host);
                Ok(())
            }
            AuthCmd::Login {
                host,
                actor_id,
                kind,
            } => {
                let path = default_auth_state_path();
                let mut state = AuthState::load(&path).into_diagnostic().wrap_err_with(|| {
                    format!("failed to load auth state from {}", path.display())
                })?;
                state.add_login(
                    &host,
                    LoginEntry {
                        actor_id: actor_id.clone(),
                        kind,
                    },
                );
                state
                    .save(&path)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to save auth state to {}", path.display()))?;
                println!(
                    "logged in as '{}' ({:?}) on host '{}'",
                    actor_id, kind, host
                );
                Ok(())
            }
            AuthCmd::List { host } => {
                let path = default_auth_state_path();
                let state = AuthState::load(&path).into_diagnostic().wrap_err_with(|| {
                    format!("failed to load auth state from {}", path.display())
                })?;
                match host {
                    Some(h) => {
                        let entries = state.list_for(&h);
                        if entries.is_empty() {
                            println!("no logins for host {}", h);
                        } else {
                            for e in entries {
                                println!("{}\t{:?}", e.actor_id, e.kind);
                            }
                        }
                    }
                    None => {
                        if state.logins.is_empty() {
                            println!("no logins recorded");
                        }
                        for (h, set) in state.logins.iter() {
                            println!("{}:", h);
                            for k in set.iter() {
                                println!("  {}\t{:?}", k.actor_id, k.kind);
                            }
                        }
                    }
                }
                Ok(())
            }
        },
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
            // Resolve the provided component path relative to cwd
            let full_path = if component.is_absolute() {
                component.clone()
            } else {
                std::env::current_dir()
                    .into_diagnostic()
                    .wrap_err("failed to get current directory")?
                    .join(&component)
            };
            let full_path = full_path
                .canonicalize()
                .into_diagnostic()
                .wrap_err_with(|| format!(
                    "failed to resolve build path '{}'; provide a valid component dir or a Cargo workspace/crate",
                    component.display()
                ))?;

            let repo_mgr = RepoManager::load()
                .into_diagnostic()
                .wrap_err("unable to open repo contexts")?;

            let has_kdl = full_path.join("package.kdl").exists();
            let has_cargo = full_path.join("Cargo.toml").exists();

            if !has_kdl && has_cargo {
                // Cargo project/workspace: derive all members and build/package each
                let derived = crate::metadata::cargo::components_from_cargo_dir(&full_path)
                    .wrap_err("failed to derive components from Cargo workspace")?;
                if derived.is_empty() {
                    return Err(miette::miette!(
                        "no cargo packages found under {}",
                        full_path.display()
                    ));
                }
                let mut first = true;
                for d in derived {
                    tracing::info!(target: "pkgdev::cli", "[pkgdev] Building cargo member: {} ({})", d.component.get_name(), d.component.get_path().display());
                    let mut ba = build_args.clone();
                    if !first {
                        // Avoid re-downloading/unpacking/build dir cleaning, but ensure we start with a clean prototype/manifest
                        ba.no_clean = true;
                        // Proactively clean prototype and manifest to avoid cross-contamination between packages
                        if let Ok(proto) = wks.get_or_create_prototype_dir() {
                            let _ = std::fs::remove_dir_all(&proto);
                        }
                        if let Ok(mani) = wks.get_or_create_manifest_dir() {
                            let _ = std::fs::remove_dir_all(&mani);
                        }
                    }
                    first = false;
                    run_build(
                        &d.component,
                        &gate,
                        &wks,
                        &settings,
                        &ba,
                        &repo_mgr,
                        args.repo.clone(),
                        args.repo_context.clone(),
                    )
                    .await
                    .wrap_err("build failed")?;
                }
                Ok(())
            } else {
                // Traditional component (package.kdl) or path that needs gate resolution
                let component =
                    open_component_local(component, &gate).wrap_err("cannot open component")?;
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
