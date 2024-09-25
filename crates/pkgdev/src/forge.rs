use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};
use miette::Diagnostic;
use reqwest::header::HeaderMap;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{ParseError, Url};

use component::{Component, ComponentError};
use forge::AuthConfig;
use gate::{Gate, GateError};

use crate::forge::api::types::ActorSshKeyFingerprint;
use crate::forge::api::{types, Client};
use crate::{get_project_dir, openid};

mod api {
    include!(concat!(env!("OUT_DIR"), "/forge.codegen.rs"));
}

#[derive(Debug, Subcommand)]
pub enum ForgeArgs {
    Connect {
        target: String,
        #[arg(short, long)]
        provider: LoginProvider,
        #[arg(short, long)]
        select: bool,
        handle: String,
        #[arg(short, long)]
        display_name: Option<String>,
    },
    DefineGate {
        #[arg(short, long)]
        file: Option<PathBuf>,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        version: Option<String>,
        #[arg(short, long)]
        branch: Option<String>,
        #[arg(short, long)]
        publisher: Option<String>,
    },
    ImportComponent {
        gate: PathBuf,
        path: PathBuf,
    },
    UploadFile {
        gate: PathBuf,
        path: PathBuf,
        kind: ComponentFileKind,
        #[arg(short, long)]
        url: Option<Url>,
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
}

#[derive(Debug, ValueEnum, Clone)]
pub enum LoginProvider {
    Github,
    Gitlab,
}

#[derive(Debug, ValueEnum, Clone, strum::Display)]
pub enum ComponentFileKind {
    Patch,
    Archive,
    Script,
}

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    ParseUrl(#[from] ParseError),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    PkgDevError(#[from] crate::Error),

    #[error("no forge connected please use forge connect first")]
    NoForgeConnected,

    #[error("missing parameter: {0}")]
    MissingParameter(String),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Gate(#[from] GateError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Component(#[from] ComponentError),

    #[error(transparent)]
    Octocrab(#[from] octocrab::Error),

    #[error("component is missing {0}")]
    ComponentIncomplete(String),

    #[error("gate document has no id defined")]
    GateNoId,

    #[error("forge has not provided any login details for the selected provider please use a connected provider for this domain")]
    OAuthProviderNotConnected,

    #[error("login aborted")]
    LoginAborted,

    #[error(transparent)]
    Progenitor(#[from] progenitor_client::Error<types::ApiError>),

    #[error("usage error either file or url must be provided")]
    UploadUsageError,
}

pub type Result<T, E = Error> = miette::Result<T, E>;

#[derive(Serialize, Deserialize)]
pub struct ForgeConfig {
    pub forges: HashMap<String, ForgeConnection>,
    pub selected_forge: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ForgeConnection {
    pub target: String,
    pub handle: String,
    pub access_token: String,
    pub refresh_token: String,
    pub actor_ssh_key_fingerprints: Vec<ActorSshKeyFingerprint>,
}

impl ForgeConnection {
    pub fn get_header(&self) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.access_token).parse().unwrap(),
        );
        map
    }
}

impl ForgeConfig {
    pub fn insert_forge(
        &mut self,
        host: String,
        target: String,
        handle: String,
        access_token: String,
        refresh_token: String,
        actor_ssh_key_fingerprints: Vec<ActorSshKeyFingerprint>,
        select: bool,
    ) {
        self.forges.insert(
            host.clone(),
            ForgeConnection {
                target,
                handle,
                access_token,
                refresh_token,
                actor_ssh_key_fingerprints,
            },
        );

        if select {
            self.selected_forge = Some(host);
        }
    }

    pub fn select_forge(&mut self, target: Option<String>) {
        if let Some(target) = target {
            for (name, _) in &self.forges {
                if &target == name {
                    self.selected_forge = Some(name.clone());
                }
            }
        } else {
            for (name, _) in &self.forges {
                self.selected_forge = Some(name.clone());
                break;
            }
        }
    }

    pub fn get_selected(&self) -> Option<Client> {
        if self.forges.len() == 0 {
            None
        } else {
            if let Some(idx) = &self.selected_forge {
                self.forges.get(idx).map(|u| {
                    let dur = std::time::Duration::from_secs(15);
                    let client = reqwest::ClientBuilder::new()
                        .default_headers(u.get_header())
                        .connect_timeout(dur)
                        .timeout(dur)
                        .build()
                        .unwrap();
                    Client::new_with_client(u.target.as_str(), client)
                })
            } else {
                self.forges
                    .clone()
                    .into_iter()
                    .collect::<Vec<(String, ForgeConnection)>>()
                    .first()
                    .map(|(_, u)| {
                        let dur = std::time::Duration::from_secs(15);
                        let client = reqwest::ClientBuilder::new()
                            .default_headers(u.get_header())
                            .connect_timeout(dur)
                            .timeout(dur)
                            .build()
                            .unwrap();
                        Client::new_with_client(u.target.as_str(), client)
                    })
            }
        }
    }

    pub fn get_selected_config(&self) -> Option<ForgeConnection> {
        if self.forges.len() == 0 {
            None
        } else {
            if let Some(idx) = &self.selected_forge {
                self.forges.get(idx).map(|v| v.clone())
            } else {
                self.forges
                    .clone()
                    .into_iter()
                    .collect::<Vec<(String, ForgeConnection)>>()
                    .first()
                    .map(|(_, v)| v.clone())
            }
        }
    }
}

pub fn get_forge_config() -> Result<ForgeConfig> {
    let project_dirs = get_project_dir()?;
    let config_dir = project_dirs.config_dir();
    let forge_config: ForgeConfig = if config_dir.join("forge.json").exists() {
        let f = File::open(config_dir.join("forge.json"))?;
        serde_json::from_reader(f)?
    } else {
        ForgeConfig {
            forges: HashMap::new(),
            selected_forge: None,
        }
    };
    Ok(forge_config)
}

pub fn save_forge_config(forge_config: &mut ForgeConfig) -> Result<(), Error> {
    let project_dirs = get_project_dir()?;
    let config_dir = project_dirs.config_dir();
    if !config_dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(config_dir)?;
    }
    let mut f = File::create(config_dir.join("forge.json"))?;
    serde_json::to_writer_pretty(&mut f, &forge_config)?;
    Ok(())
}

pub async fn handle_forge_interaction(args: &ForgeArgs) -> Result<()> {
    let mut forge_config = get_forge_config()?;
    let forge_client = forge_config.get_selected();

    match args {
        ForgeArgs::DefineGate {
            file,
            name,
            version,
            branch,
            publisher,
        } => {
            if forge_client.is_none() {
                return Err(Error::NoForgeConnected);
            }
            let forge_client = forge_client.unwrap();
            let gate: Option<Gate> = if let Some(file) = file {
                if file.exists() {
                    Some(Gate::new(file)?)
                } else {
                    Some(Gate::empty(file)?)
                }
            } else {
                None
            };

            let (name, version, branch, publisher, transforms) =
                if name.is_some() && version.is_some() && branch.is_some() && publisher.is_some() {
                    // We define everything on the commandline and thus use those variables.
                    (
                        name.clone().unwrap(),
                        version.clone().unwrap(),
                        branch.clone().unwrap(),
                        publisher.clone().unwrap(),
                        None,
                    )
                } else if let Some(gate) = gate {
                    // take the variables from the file
                    (
                        gate.name,
                        gate.version,
                        gate.branch,
                        gate.publisher,
                        Some(
                            gate.default_transforms
                                .iter()
                                .map(|t| t.to_string())
                                .collect::<Vec<String>>(),
                        ),
                    )
                } else {
                    // All other cases are user error
                    return Err(Error::MissingParameter(String::from("name")));
                };

            forge_client
                .create_gate(&types::CreateGateInput {
                    name,
                    version,
                    branch,
                    publisher,
                    transforms,
                })
                .await?;

            Ok(())
        }
        ForgeArgs::Connect {
            target,
            select,
            provider,
            handle,
            display_name,
        } => {
            let target_url: Url = target.parse()?;
            let host = target_url.host_str();
            if host.is_none() {
                return Err(Error::MissingParameter(String::from(
                    "no host in forge URL",
                )));
            }

            let host = host.unwrap();

            let login_info = get_oauth_login_info(&target_url).await?;

            let token = openid::login_to_provider(provider, &login_info).await?;
            let forge_client = Client::new(target);

            let resp = forge_client
                .actor_connect(&types::ActorConnectRequest::GitHub {
                    display_name: display_name.clone(),
                    handle: handle.to_string(),
                    ssh_keys: vec![],
                    token: token.access_token.expose_secret().clone(),
                })
                .await?;

            forge_config.insert_forge(
                host.to_string(),
                target.clone(),
                resp.handle.clone(),
                resp.access_token.clone(),
                resp.refresh_token.clone(),
                resp.ssh_keys.clone(),
                *select,
            );

            save_forge_config(&mut forge_config)?;

            println!("connected");

            Ok(())
        }
        ForgeArgs::ImportComponent { gate, path } => {
            if forge_client.is_none() {
                return Err(Error::NoForgeConnected);
            }
            let forge_client = forge_client.unwrap();
            let gate = Gate::new(gate)?;
            let component = Component::open_local(path)?;

            let gate_id = gate.id.ok_or(Error::GateNoId)?;

            let (anitya_id, repology_id) = if let Some(metadata) = &component.recipe.metadata {
                let mut anytia_id = None;
                let mut repology_id = None;
                for item in &metadata.0 {
                    match item.name.as_str() {
                        "anitya_id" => anytia_id = Some(item.value.clone()),
                        "repology_id" => repology_id = Some(item.value.clone()),
                        _ => {}
                    }
                }
                (anytia_id, repology_id)
            } else {
                (None, None)
            };

            let recipe_value = serde_json::to_value(&component.recipe)?;
            let package_meta_value =
                serde_json::to_value(&component.package_meta.unwrap_or_default())?;

            forge_client
                .import_component(&types::ComponentInput {
                    anitya_id,
                    gate: gate_id,
                    packages: serde_json::from_value(package_meta_value)?,
                    recipe: serde_json::from_value(recipe_value)?,
                    repology_id,
                })
                .await?;

            Ok(())
        }
        ForgeArgs::UploadFile {
            gate,
            path,
            kind,
            url,
            file,
        } => {
            if forge_client.is_none() {
                return Err(Error::NoForgeConnected);
            }
            let forge_client = forge_client.unwrap();
            let gate = Gate::new(gate)?;
            let component = Component::open_local(path)?;

            let gate_id = gate.id.ok_or(Error::GateNoId)?;

            let name = component.recipe.name;
            let version = component
                .recipe
                .version
                .ok_or(Error::ComponentIncomplete(String::from("version")))?;
            let revision = component.recipe.revision.unwrap_or(String::from("0"));

            let identifier = types::ComponentIdentifier {
                gate_id,
                name,
                revision,
                version,
            };

            let ident_str = serde_json::to_string(&identifier)?;

            let resp = if let Some(file) = file {
                let client = reqwest::Client::new();

                let async_file = tokio::fs::File::open(file).await?;

                let some_file = reqwest::multipart::Part::stream(async_file);

                let form = reqwest::multipart::Form::new()
                    .text("identifier", ident_str)
                    .part(file.to_string_lossy().to_string(), some_file);

                client
                    .post(format!(
                        "{}/api/v1/components/upload/{kind}",
                        forge_client.baseurl()
                    ))
                    .multipart(form)
                    .send()
                    .await?
            } else if let Some(url) = url {
                let client = reqwest::Client::new();

                let form = reqwest::multipart::Form::new()
                    .text("identifier", ident_str)
                    .text("url", url.to_string());

                client
                    .post(format!(
                        "{}/api/v1/components/upload/{kind}",
                        forge_client.baseurl()
                    ))
                    .multipart(form)
                    .send()
                    .await?
            } else {
                return Err(Error::UploadUsageError);
            };

            if resp.status().is_success() {
                println!("Upload successful");
            } else {
                let err: types::ApiError = resp.json().await?;
                println!("error while uploading file: {:#?}", err);
            }

            Ok(())
        }
    }
}

pub async fn get_oauth_login_info(target: &Url) -> Result<AuthConfig, Error> {
    let resp = reqwest::get(target.join("/api/v1/auth/login_info")?).await?;

    let login_info: AuthConfig = resp.json().await?;
    Ok(login_info)
}
