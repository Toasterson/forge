use std::fs;
use std::path::{Path, PathBuf};

use crate::api::forged::api::v1 as api;
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

use crate::api::forged::api::v2 as api_v2;
use crate::auth::{
    authenticated_request, connect_grpc, default_auth_state_path, diagnose_rpc_error,
    get_valid_token, login_device_flow, print_token_status, server_url_from_host, ActorKind,
    AuthClient, AuthState, LoginEntry, TokenStore,
};
use crate::component_client::ComponentClient;
use crate::gate_client::GateClient;

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

    /// Skip TLS certificate verification (for testing with staging certificates).
    /// NOTE: Currently not functional — reserved for future use when tonic supports
    /// accepting invalid certificates.
    #[arg(long = "tls-insecure", global = true, hide = true)]
    pub tls_insecure: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(name = "repo")]
    Repo {
        #[command(subcommand)]
        cmd: RepoCmd,
    },
    #[command(name = "forge")]
    /// Interact with the forge.
    Forge {
        #[command(subcommand)]
        cmd: ForgeCmd,
    },
    #[command(name = "download")]
    Download {
        /// Component folder path relative to the gate's components directory (e.g., `ffmpeg` or `web/firefox`).
        /// If omitted, current directory is used. Absolute paths are accepted.
        #[arg(short, long, default_value = ".")]
        component: PathBuf,
    },
    #[command(name = "metadata")]
    Metadata {
        #[command(flatten)]
        args: ComponentArgs,
        #[arg(default_value_t = metadata::MetadataFormat::default())]
        format: metadata::MetadataFormat,
    },
    #[command(name = "generate")]
    Generate {
        #[arg(default_value_t = GenerateSchemaKind::default())]
        kind: GenerateSchemaKind,
        /// Output file path for generated data (stdout if omitted)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    #[command(name = "create")]
    Create {
        fmri: String,
        #[command(flatten)]
        args: ComponentArgs,
    },
    #[command(name = "edit")]
    Edit {
        /// Component folder path relative to the gate's components directory (e.g., `ffmpeg` or `web/firefox`).
        /// If omitted, current directory is used. Absolute paths are accepted.
        #[arg(short, long, default_value = ".")]
        component: PathBuf,
        #[command(subcommand)]
        args: EditArgs,
    },
    #[command(name = "auth")]
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    #[command(name = "build")]
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
    #[arg(short, long, default_value = ".")]
    pub component: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum GateCmd {
    /// Open (create or update) a gate on the forge
    Open {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Gate identifier (stable id)
        #[arg(long)]
        id: String,
        /// Human-friendly name for the gate
        #[arg(long)]
        name: Option<String>,
        /// Owner actor id; if omitted, will use the selected login for this host (or first recorded login)
        #[arg(long)]
        owner_id: Option<String>,
        /// Owner actor kind (defaults to User)
        #[arg(long, value_enum)]
        owner_kind: Option<ActorKind>,
    },
    /// Upload local gate metadata to the forge (upsert)
    Upload {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Owner actor id; if omitted, will use the selected login for this host (or first recorded login)
        #[arg(long)]
        owner_id: Option<String>,
        /// Owner actor kind (defaults to User)
        #[arg(long, value_enum)]
        owner_kind: Option<ActorKind>,
    },
    /// List all gates on the forge
    List {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Do not print the header line
        #[arg(long = "no-header")]
        no_header: bool,
    },
    /// Show details for a specific gate
    Show {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Gate identifier
        #[arg(long)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ComponentCmd {
    /// Create (upsert) a component on the forge
    Create {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Component identifier (stable id)
        #[arg(long)]
        id: String,
        /// Human-friendly name for the component
        #[arg(long)]
        name: Option<String>,
    },
    /// Upload local component metadata to the forge (upsert)
    Upload {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Component folder path relative to the gate's components directory (e.g., `ffmpeg` or `web/firefox`).
        /// If omitted, current directory is used. Absolute paths are accepted.
        #[arg(value_name = "COMPONENT", index = 1, default_value = ".")]
        component: PathBuf,
    },
    /// List components available on the forge
    List {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Do not print the header line
        #[arg(long = "no-header")]
        no_header: bool,
    },
    /// Show details for a specific component, rendering its package.kdl summary from the forge
    Show {
        /// Forge hostname (or hostname:port). If omitted, uses selected context.
        #[arg(long)]
        host: Option<String>,
        /// Component identifier (positional)
        #[arg(value_name = "ID", index = 1)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ForgeCmd {
    /// Manage gates on the forge server
    Gate {
        #[command(subcommand)]
        cmd: GateCmd,
    },
    /// Manage components on the forge server
    Component {
        #[command(subcommand)]
        cmd: ComponentCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthCmd {
    /// Authenticate with a forge server using OAuth 2.0 Device Authorization Grant (RFC 8628).
    /// Opens the OIDC provider in your browser for interactive login.
    Login {
        /// Forge hostname (or hostname:port) to authenticate against
        #[arg(long)]
        host: String,
        /// Also select this login as the default context for future commands
        #[arg(long)]
        select: bool,
    },
    /// Show the current token status for stored OAuth sessions
    Status {
        /// Forge hostname (or hostname:port). Shows all hosts if omitted.
        #[arg(long)]
        host: Option<String>,
    },
    /// Remove stored OAuth tokens for a forge host (log out)
    Logout {
        /// Forge hostname (or hostname:port) to remove tokens for
        #[arg(long)]
        host: String,
    },
    /// Register a new actor with their SSH public key on a forge.
    /// Requires an active OAuth session (run 'auth login' first).
    /// Add an SSH public key to your OIDC-authenticated account.
    /// Requires a prior `auth login`.
    #[command(name = "add-key")]
    AddKey {
        /// Forge hostname
        #[arg(long)]
        host: Option<String>,
        /// Path to the SSH public key file (OpenSSH format)
        #[arg(long)]
        public_key: PathBuf,
        /// Human-readable label for the key (e.g., "laptop", "ci")
        #[arg(long, default_value = "default")]
        key_id: String,
    },
    /// Confirm a pending registration using the age-encrypted envelope from the email
    Confirm {
        #[arg(long)]
        host: String,
        #[arg(long)]
        actor_id: String,
        #[arg(long, value_enum, default_value_t = ActorKind::User)]
        kind: ActorKind,
        /// Age-encrypted envelope ciphertext (Base64-URL, no padding) from the email
        #[arg(long)]
        envelope: String,
        /// Path to your SSH private key for decrypting --envelope (defaults to ~/.ssh/id_ed25519)
        #[arg(long)]
        identity: Option<PathBuf>,
        /// After confirming, record a local login for this host/actor
        #[arg(long)]
        set_login: bool,
        /// Also select this login as the default context
        #[arg(long)]
        select: bool,
    },
    /// List current logins. If --host is provided, only show that forge.
    List {
        #[arg(long)]
        host: Option<String>,
    },
    /// Select the default login context (host + actor)
    Select {
        /// Forge hostname (or hostname:port)
        #[arg(long)]
        host: String,
        /// Actor identifier (username@domain)
        #[arg(long)]
        actor_id: String,
        /// Actor kind
        #[arg(long, value_enum, default_value_t = ActorKind::User)]
        kind: ActorKind,
    },
}

#[derive(Debug, Subcommand)]
pub enum RepoCmd {
    #[command(name = "list")]
    List,
    #[command(name = "create")]
    Create { name: String, path: Option<PathBuf> },
    #[command(name = "delete")]
    Delete { name: String },
    #[command(name = "select")]
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
            AuthCmd::Login { host, select: _ } => {
                let token_set = login_device_flow(&host, args.tls_insecure)
                    .await
                    .wrap_err("device authorization flow failed")?;

                // Store the token
                let mut store = TokenStore::load();
                store.set(host.clone(), token_set);
                store.save().wrap_err("failed to save token")?;

                // Always set this host as the selected context
                {
                    let path = default_auth_state_path();
                    let mut state = AuthState::load(&path).unwrap_or_default();
                    state.set_selected(host.clone(), host.clone(), ActorKind::User);
                    let _ = state.save(&path);
                }

                println!("authenticated successfully on {}", host);
                Ok(())
            }
            AuthCmd::Status { host } => {
                print_token_status(host.as_deref());
                Ok(())
            }
            AuthCmd::Logout { host } => {
                let mut store = TokenStore::load();
                if store.remove(&host).is_some() {
                    store.save().wrap_err("failed to save token store")?;
                    println!("logged out from {}", host);
                } else {
                    println!("no stored token for host '{}'", host);
                }
                Ok(())
            }
            AuthCmd::AddKey {
                host,
                public_key,
                key_id,
            } => {
                let host = resolve_host_or_selected(host)?;
                let token = get_valid_token(&host)
                    .await
                    .wrap_err("authentication required — run 'pkgdev auth login' first")?;

                // Read SSH public key
                let key_data = std::fs::read_to_string(&public_key).map_err(|e| {
                    miette::miette!(
                        "failed to read public key file '{}': {}",
                        public_key.display(),
                        e
                    )
                })?;

                let grpc_url = server_url_from_host(&host);
                let channel = connect_grpc(&grpc_url, args.tls_insecure).await?;
                let mut client = api_v2::auth_service_client::AuthServiceClient::new(channel);
                let req = authenticated_request(
                    api_v2::RegisterActorRequest {
                        display_name: String::new(), // ignored, identity from OIDC token
                        email: String::new(),        // ignored
                        public_key: key_data.trim().to_string(),
                        key_id: key_id.clone(),
                    },
                    &token,
                );
                client
                    .register_actor(req)
                    .await
                    .map_err(|e| diagnose_rpc_error(&grpc_url, "RegisterActor", e))?;

                println!("SSH key '{}' added to your account on {}", key_id, host);
                Ok(())
            }
            AuthCmd::Confirm {
                host,
                actor_id,
                kind,
                envelope,
                identity,
                set_login,
                select,
            } => {
                let token = get_valid_token(&host)
                    .await
                    .wrap_err("authentication required before confirmation")?;
                let url = server_url_from_host(&host);
                let client = AuthClient::connect(url)
                    .await
                    .wrap_err("failed to connect to forge host")?;

                let identity_path = identity.unwrap_or_else(|| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                    PathBuf::from(format!("{}/.ssh/id_ed25519", home))
                });
                client
                    .confirm_registration_encrypted(
                        actor_id.clone(),
                        kind,
                        &envelope,
                        &identity_path,
                        &token,
                    )
                    .await
                    .wrap_err("confirmation RPC failed (decrypt)")?;

                // Optionally record login and select context
                if set_login || select {
                    let path = default_auth_state_path();
                    let mut state =
                        AuthState::load(&path).into_diagnostic().wrap_err_with(|| {
                            format!("failed to load auth state from {}", path.display())
                        })?;
                    if set_login {
                        state.add_login(
                            &host,
                            LoginEntry {
                                actor_id: actor_id.clone(),
                                kind,
                            },
                        );
                    }
                    if select {
                        state.set_selected(host.clone(), actor_id.clone(), kind);
                    }
                    state.save(&path).into_diagnostic().wrap_err_with(|| {
                        format!("failed to save auth state to {}", path.display())
                    })?;
                }
                println!("registration confirmation sent on {}", host);
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
            AuthCmd::Select {
                host,
                actor_id,
                kind,
            } => {
                let path = default_auth_state_path();
                let mut state = AuthState::load(&path).into_diagnostic().wrap_err_with(|| {
                    format!("failed to load auth state from {}", path.display())
                })?;
                // Validate the login exists for the host
                let exists = state
                    .list_for(&host)
                    .into_iter()
                    .any(|e| e.actor_id == actor_id && e.kind == kind);
                if !exists {
                    return Err(miette::miette!(
                        "no such login for host '{}': {} ({:?}). Use 'pkgdev auth login --host {}' first",
                        host,
                        actor_id,
                        kind,
                        host,
                    ));
                }
                state.set_selected(host.clone(), actor_id.clone(), kind);
                state
                    .save(&path)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to save auth state to {}", path.display()))?;
                println!(
                    "selected login '{}' ({:?}) on host '{}'",
                    actor_id, kind, host
                );
                Ok(())
            }
        },
        Commands::Forge { cmd } => {
            match cmd {
                ForgeCmd::Gate { cmd } => match cmd {
                    GateCmd::Open {
                        host,
                        id,
                        name,
                        owner_id,
                        owner_kind,
                    } => {
                        let host = resolve_host_or_selected(host)?;
                        let token = get_valid_token(&host)
                            .await
                            .wrap_err("authentication required")?;
                        let url = server_url_from_host(&host);
                        let client = GateClient::connect(url)
                            .await
                            .wrap_err("failed to connect to forge host")?;
                        let (oid, okind) = resolve_owner(&host, owner_id, owner_kind)?;
                        let gate = api::Gate {
                            id,
                            name: name.unwrap_or_default(),
                            owner: Some(api::ActorRef {
                                id: oid,
                                kind: actor_kind_str(okind),
                            }),
                            members: vec![],
                        };
                        let created = client
                            .create_gate(gate, &token)
                            .await
                            .wrap_err("create gate RPC failed")?;
                        println!("gate '{}' created on {}", created.id, host);
                        Ok(())
                    }
                    GateCmd::Upload {
                        host,
                        owner_id,
                        owner_kind,
                    } => {
                        let Some(g) = &gate else {
                            return Err(miette::miette!("--gate must be provided for 'forge gate upload' or run in a gate directory"));
                        };
                        let host = resolve_host_or_selected(host)?;
                        let token = get_valid_token(&host)
                            .await
                            .wrap_err("authentication required")?;
                        let url = server_url_from_host(&host);
                        let client = GateClient::connect(url)
                            .await
                            .wrap_err("failed to connect to forge host")?;
                        let (oid, okind) = resolve_owner(&host, owner_id, owner_kind)?;
                        let id = g.id.clone().unwrap_or_else(|| g.name.clone());
                        let gate_msg = api::Gate {
                            id,
                            name: g.name.clone(),
                            owner: Some(api::ActorRef {
                                id: oid,
                                kind: actor_kind_str(okind),
                            }),
                            members: vec![],
                        };
                        let created = client
                            .create_gate(gate_msg, &token)
                            .await
                            .wrap_err("upload gate RPC failed")?;
                        println!("gate '{}' uploaded to {}", created.id, host);
                        Ok(())
                    }
                    GateCmd::List { host, no_header } => {
                        let host = resolve_host_or_selected(host)?;
                        let token = get_valid_token(&host)
                            .await
                            .wrap_err("authentication required")?;
                        let url = server_url_from_host(&host);
                        let client = GateClient::connect(url)
                            .await
                            .wrap_err("failed to connect to forge host")?;
                        let gates = client
                            .list_gates(&token)
                            .await
                            .wrap_err("list gates RPC failed")?;
                        if gates.is_empty() {
                            println!("no gates on {}", host);
                        } else {
                            if !no_header {
                                println!("ID\tNAME\tOWNER_ID\tOWNER_KIND");
                            }
                            for g in gates {
                                let (owner_id, owner_kind) = if let Some(o) = g.owner {
                                    (o.id, o.kind)
                                } else {
                                    (String::new(), String::new())
                                };
                                println!("{}\t{}\t{}\t{}", g.id, g.name, owner_id, owner_kind);
                            }
                        }
                        Ok(())
                    }
                    GateCmd::Show { host, id } => {
                        let host = resolve_host_or_selected(host)?;
                        let token = get_valid_token(&host)
                            .await
                            .wrap_err("authentication required")?;
                        let url = server_url_from_host(&host);
                        let client = GateClient::connect(url)
                            .await
                            .wrap_err("failed to connect to forge host")?;
                        match client
                            .get_gate(&id, &token)
                            .await
                            .wrap_err("get gate RPC failed")?
                        {
                            None => {
                                println!("gate '{}' not found on {}", id, host);
                            }
                            Some(g) => {
                                println!("id: {}", g.id);
                                if !g.name.is_empty() {
                                    println!("name: {}", g.name);
                                }
                                if let Some(o) = g.owner {
                                    println!("owner: {} {}", o.kind, o.id);
                                }
                                if g.members.is_empty() {
                                    println!("members: 0");
                                } else {
                                    println!("members:");
                                    for m in g.members {
                                        if let Some(ar) = m.actor.as_ref() {
                                            let roles = if m.roles.is_empty() {
                                                String::from("[]")
                                            } else {
                                                format!("[{}]", m.roles.join(","))
                                            };
                                            let perms = if m.permissions.is_empty() {
                                                String::from("[]")
                                            } else {
                                                format!("[{}]", m.permissions.join(","))
                                            };
                                            println!(
                                                "- {} {} roles:{} perms:{}",
                                                ar.kind, ar.id, roles, perms
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Ok(())
                    }
                },
                ForgeCmd::Component { cmd } => {
                    match cmd {
                        ComponentCmd::Create { host, id, name } => {
                            let host = resolve_host_or_selected(host)?;
                            let token = get_valid_token(&host)
                                .await
                                .wrap_err("authentication required")?;
                            let url = server_url_from_host(&host);
                            let client = ComponentClient::connect(url)
                                .await
                                .wrap_err("failed to connect to forge host")?;
                            let comp = api::Component {
                                id,
                                name: name.unwrap_or_default(),
                                files: None,
                                base_json: String::new(),
                            };
                            let created = client
                                .create_component(comp, &token)
                                .await
                                .wrap_err("create component RPC failed")?;
                            println!("component '{}' created on {}", created.id, host);
                            Ok(())
                        }
                        ComponentCmd::Upload { host, component } => {
                            let host = resolve_host_or_selected(host)?;
                            let token = get_valid_token(&host)
                                .await
                                .wrap_err("authentication required")?;
                            let url = server_url_from_host(&host);
                            let client = ComponentClient::connect(url)
                                .await
                                .wrap_err("failed to connect to forge host")?;
                            let comp_local = open_component_local(&component, &gate)
                                .wrap_err("cannot open component")?;
                            let id = comp_local.get_name().to_string();
                            let base_json = serde_json::to_string(&comp_local)
                                .into_diagnostic()
                                .wrap_err("serialize component for upload")?;
                            let comp_msg = api::Component {
                                id: id.clone(),
                                name: comp_local.get_name().to_string(),
                                files: None,
                                base_json,
                            };
                            let created = client
                                .create_component(comp_msg, &token)
                                .await
                                .wrap_err("upload component RPC failed")?;
                            println!("component '{}' uploaded to {}", created.id, host);
                            Ok(())
                        }
                        ComponentCmd::List { host, no_header } => {
                            let host = resolve_host_or_selected(host)?;
                            let token = get_valid_token(&host)
                                .await
                                .wrap_err("authentication required")?;
                            let url = server_url_from_host(&host);
                            let client = ComponentClient::connect(url)
                                .await
                                .wrap_err("failed to connect to forge host")?;
                            let components = client
                                .list_components(&token)
                                .await
                                .wrap_err("list components RPC failed")?;
                            if components.is_empty() {
                                println!("no components on {}", host);
                            } else {
                                if !no_header {
                                    println!("ID\tNAME");
                                }
                                for c in components {
                                    println!("{}\t{}", c.id, c.name);
                                }
                            }
                            Ok(())
                        }
                        ComponentCmd::Show { host, id } => {
                            let host = resolve_host_or_selected(host)?;
                            let token = get_valid_token(&host)
                                .await
                                .wrap_err("authentication required")?;
                            let url = server_url_from_host(&host);
                            let client = ComponentClient::connect(url)
                                .await
                                .wrap_err("failed to connect to forge host")?;

                            let remote = client
                                .get_component(&id, &token)
                                .await
                                .wrap_err("get component RPC failed")?;

                            println!("Component: {}", id);
                            if let Some(rc) = &remote {
                                println!("  Name: {}", rc.name);
                                if let Some(files) = rc.files.as_ref() {
                                    if files.patches.is_empty() {
                                        println!("  Patches: 0");
                                    } else {
                                        println!("  Patches ({}):", files.patches.len());
                                        for f in &files.patches {
                                            println!("    - {} ({})", f.name, f.rel_path);
                                        }
                                    }
                                    if files.licenses.is_empty() {
                                        println!("  Licenses: 0");
                                    } else {
                                        println!("  Licenses ({}):", files.licenses.len());
                                        for f in &files.licenses {
                                            println!("    - {} ({})", f.name, f.rel_path);
                                        }
                                    }
                                    if files.scripts.is_empty() {
                                        println!("  Scripts: 0");
                                    } else {
                                        println!("  Scripts ({}):", files.scripts.len());
                                        for f in &files.scripts {
                                            println!("    - {} ({})", f.name, f.rel_path);
                                        }
                                    }
                                }

                                // Render package.kdl description from the forge (base_json)
                                if !rc.base_json.is_empty() {
                                    match serde_json::from_str::<Component>(&rc.base_json) {
                                        Ok(model) => {
                                            println!("\nPackage KDL (describe) [from forge]:");
                                            println!("  Name: {}", model.get_name());
                                            let r = &model.recipe;
                                            if let Some(v) = &r.version {
                                                println!("  Version: {}", v);
                                            }
                                            if let Some(rev) = &r.revision {
                                                println!("  Revision: {}", rev);
                                            }
                                            if let Some(s) = &r.summary {
                                                println!("  Summary: {}", s);
                                            }
                                            if let Some(u) = &r.project_url {
                                                println!("  Project URL: {}", u);
                                            }
                                            if let Some(l) = &r.license {
                                                println!("  License: {}", l);
                                            }
                                            if let Some(c) = &r.classification {
                                                println!("  Classification: {}", c);
                                            }
                                            if !r.maintainers.is_empty() {
                                                println!(
                                                    "  Maintainers ({}):",
                                                    r.maintainers.len()
                                                );
                                                for m in &r.maintainers {
                                                    println!("    - {}", m);
                                                }
                                            }
                                            if r.sources.is_empty() {
                                                println!("  Sources: 0");
                                            } else {
                                                println!(
                                                    "  Sources ({}):",
                                                    r.sources
                                                        .iter()
                                                        .map(|s| s.sources.len())
                                                        .sum::<usize>()
                                                );
                                                for (i, section) in r.sources.iter().enumerate() {
                                                    for src in &section.sources {
                                                        match src {
                                                            SourceNode::Archive(a) => {
                                                                println!(
                                                                    "    - archive: {}",
                                                                    a.src
                                                                );
                                                            }
                                                            other => {
                                                                println!("    - {:?}", other);
                                                            }
                                                        }
                                                    }
                                                    if i + 1 < r.sources.len() {
                                                        // spacer between sections
                                                    }
                                                }
                                            }
                                            if r.build_sections.is_empty() {
                                                println!("  Build: 0 sections");
                                            } else {
                                                println!(
                                                    "  Build sections ({}):",
                                                    r.build_sections.len()
                                                );
                                                for (idx, b) in r.build_sections.iter().enumerate()
                                                {
                                                    let mut kinds: Vec<&str> = Vec::new();
                                                    if b.cargo.is_some() {
                                                        kinds.push("cargo");
                                                    }
                                                    if b.script.is_some() {
                                                        kinds.push("script");
                                                    }
                                                    if b.configure.is_some() {
                                                        kinds.push("configure");
                                                    }
                                                    if b.cmake.is_some() {
                                                        kinds.push("cmake");
                                                    }
                                                    if b.meson.is_some() {
                                                        kinds.push("meson");
                                                    }
                                                    if kinds.is_empty() {
                                                        kinds.push("custom");
                                                    }
                                                    println!(
                                                        "    - [{}] {}",
                                                        idx + 1,
                                                        kinds.join(", ")
                                                    );
                                                }
                                            }
                                        }
                                        Err(_) => {
                                            println!("\nPackage KDL (describe): unable to parse stored base component");
                                        }
                                    }
                                } else {
                                    println!("\nPackage KDL (describe): no base information stored on server");
                                }
                            } else {
                                println!("  Not found on host {}", host);
                            }

                            Ok(())
                        }
                    }
                }
            }
        }
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

// ---- Gate CLI helpers ----
fn resolve_host_or_selected(host_arg: Option<String>) -> miette::Result<String> {
    if let Some(h) = host_arg {
        return Ok(h);
    }
    // Check selected context in auth state
    let path = default_auth_state_path();
    if let Ok(state) = AuthState::load(&path).into_diagnostic() {
        if let Some(sel) = state.get_selected() {
            return Ok(sel.host.clone());
        }
    }
    // Fall back to token store — if there's exactly one host stored, use it
    let store = TokenStore::load();
    let hosts = store.hosts();
    if hosts.len() == 1 {
        return Ok(hosts[0].to_string());
    }
    Err(miette::miette!(
        "--host not provided and no selected context found.\n\
         Use 'pkgdev auth login --host <url> --select' or specify --host."
    ))
}

fn actor_kind_str(k: ActorKind) -> String {
    match k {
        ActorKind::User => "user".to_string(),
        ActorKind::Service => "service".to_string(),
    }
}

fn resolve_owner(
    host: &str,
    owner_id: Option<String>,
    owner_kind: Option<ActorKind>,
) -> miette::Result<(String, ActorKind)> {
    if let Some(id) = owner_id {
        return Ok((id, owner_kind.unwrap_or(ActorKind::User)));
    }
    let path = default_auth_state_path();
    let state = AuthState::load(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to load auth state from {}", path.display()))?;
    // Prefer the selected context for this host, if any
    if let Some(sel) = state.get_selected() {
        if sel.host == host {
            return Ok((sel.actor_id.clone(), sel.kind));
        }
    }
    // Fallback to first recorded login for host
    let entries = state.list_for(host);
    if let Some(entry) = entries.first() {
        return Ok((entry.actor_id.clone(), entry.kind));
    }
    Err(miette::miette!(
        "owner-id not provided and no local login found for host '{}'",
        host
    ))
}
