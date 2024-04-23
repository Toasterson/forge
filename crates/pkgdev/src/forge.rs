use crate::openid::OAuthConfig;
use crate::{get_project_dir, openid};
use clap::{Subcommand, ValueEnum};
use component::{Component, ComponentError, PackageMeta, Recipe};
use forge::AuthConfig;
use gate::{Gate, GateError};
use graphql_client::{GraphQLQuery, Response};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use thiserror::Error;
use url::{ParseError, Url};

#[derive(Debug, Subcommand)]
pub enum ForgeArgs {
    Connect {
        target: Url,
        #[arg(short, long)]
        provider: LoginProvider,
        #[arg(short, long)]
        select: bool,
        /// Useful for debugging. Do not contact forge at target but set the target as header and contact this Url instead.
        #[arg(long, hide(true))]
        target_override: Option<Url>,
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

#[derive(Debug, ValueEnum, Clone)]
pub enum ComponentFileKind {
    Patch,
    Archive,
    Script,
}

impl Into<upload_component_file::ComponentFileKind> for &ComponentFileKind {
    fn into(self) -> upload_component_file::ComponentFileKind {
        match self {
            ComponentFileKind::Patch => upload_component_file::ComponentFileKind::PATCH,
            ComponentFileKind::Archive => upload_component_file::ComponentFileKind::ARCHIVE,
            ComponentFileKind::Script => upload_component_file::ComponentFileKind::SCRIPT,
        }
    }
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

    #[error("forge has no provided no login details for the selected provider please use a connected provider fir this domain")]
    OAuthProviderNotConnected,

    #[error("login aborted")]
    LoginAborted,
}

pub type Result<T, E = Error> = miette::Result<T, E>;

#[derive(Serialize, Deserialize)]
pub struct ForgeConfig {
    pub forges: HashMap<String, ForgeConnection>,
    pub selected_forge: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ForgeConnection {
    pub target: Url,
    pub login_token: openid::OAuthConfig,
}

impl ForgeConfig {
    pub fn insert_forge(
        &mut self,
        host: String,
        target: Url,
        login_token: OAuthConfig,
        select: bool,
    ) {
        self.forges.insert(
            host.clone(),
            ForgeConnection {
                target,
                login_token,
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

    pub fn update_token(&mut self, token: &OAuthConfig) {
        if let Some(selected) = &self.selected_forge {
            if let Some(conn) = self.forges.get_mut(selected) {
                conn.login_token = token.clone();
            }
        }
    }

    pub fn get_selected(&self) -> Option<Url> {
        if self.forges.len() == 0 {
            None
        } else {
            if let Some(idx) = &self.selected_forge {
                self.forges.get(idx).map(|u| u.target.clone())
            } else {
                self.forges
                    .clone()
                    .into_iter()
                    .collect::<Vec<(String, ForgeConnection)>>()
                    .first()
                    .map(|(_, u)| u.target.clone())
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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentData {
    pub recipe: Recipe,
    pub packages: PackageMeta,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Upload(usize);

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "forge.graphql.schema.json",
    query_path = "queries/defineGate.graphql",
    response_derives = "Debug,Serialize,PartialEq"
)]
pub struct DefineGateMutation;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "forge.graphql.schema.json",
    query_path = "queries/importComponent.graphql",
    response_derives = "Debug,Serialize,PartialEq"
)]
pub struct ImportComponentMutation;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "forge.graphql.schema.json",
    query_path = "queries/importComponent.graphql",
    response_derives = "Debug,Serialize,PartialEq"
)]
pub struct UploadComponentFile;

pub async fn handle_forge_interaction(args: &ForgeArgs) -> Result<()> {
    let mut forge_config = get_forge_config()?;
    let forge_url = forge_config.get_selected();

    match args {
        ForgeArgs::DefineGate {
            file,
            name,
            version,
            branch,
            publisher,
        } => {
            if forge_url.is_none() {
                return Err(Error::NoForgeConnected);
            }
            let forge_url = forge_url.unwrap();
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
                        Some(gate.default_transforms),
                    )
                } else {
                    // All other cases are user error
                    return Err(Error::MissingParameter(String::from("name")));
                };

            let variables: define_gate_mutation::Variables = define_gate_mutation::Variables {
                name,
                version,
                branch,
                publisher,
                transforms: transforms
                    .unwrap_or(vec![])
                    .into_iter()
                    .map(|t| t.to_string())
                    .collect(),
            };

            let request_body = DefineGateMutation::build_query(variables);
            let client = reqwest::Client::new();
            let res = client.post(forge_url).json(&request_body).send().await?;
            let response_body: Response<define_gate_mutation::ResponseData> = res.json().await?;
            if let Some(data) = response_body.data {
                let mut gate = Gate::empty(file.clone().unwrap_or(PathBuf::from("./gate.kdl")))?;
                gate.id = Some(data.create_gate.id);
                gate.name = data.create_gate.name;
                gate.version = data.create_gate.version;
                gate.branch = data.create_gate.branch;
                gate.publisher = data.create_gate.publisher;
                //gate.default_transforms = data.create_gate.transforms.iter().map(|s| s.clone().into()).collect();
                gate.save()?;
            } else {
                let errors = response_body.errors.unwrap();
                for error in errors {
                    println!("the server returned errors");
                    println!("{}", error);
                }
            }
            Ok(())
        }
        ForgeArgs::Connect {
            target,
            select,
            provider,
            target_override,
        } => {
            let host = target.host_str();
            if host.is_none() {
                return Err(Error::MissingParameter(String::from(
                    "no host in forge URL",
                )));
            }

            let host = host.unwrap();
            let scheme = target.scheme();

            let login_info = get_oauth_login_info(target_override.clone(), host, scheme).await?;

            let token = openid::login_to_provider(provider, &login_info).await?;

            forge_config.insert_forge(host.to_string(), target.clone(), token.into(), *select);

            save_forge_config(&mut forge_config)?;

            Ok(())
        }
        ForgeArgs::ImportComponent { gate, path } => {
            if forge_url.is_none() {
                return Err(Error::NoForgeConnected);
            }
            let forge_url = forge_url.unwrap();
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

            let vars = import_component_mutation::Variables {
                anitya_id,
                data: ComponentData {
                    recipe: component.recipe,
                    packages: component
                        .package_meta
                        .ok_or(Error::ComponentIncomplete(String::from("pkg5")))?,
                },
                repology_id,
                gate: gate_id,
            };

            let request_body = ImportComponentMutation::build_query(vars);
            let client = reqwest::Client::new();
            let res = client.post(forge_url).json(&request_body).send().await?;
            let response_body: Response<import_component_mutation::ResponseData> =
                res.json().await?;

            if let Some(data) = response_body.data {
                println!(
                    "Component {}@{} revision: {} in gate {} imported",
                    data.import_component.name,
                    data.import_component.version,
                    data.import_component.revision,
                    data.import_component.gate_id,
                );
            } else {
                let errors = response_body.errors.unwrap();
                for error in errors {
                    println!("the server returned errors");
                    println!("{}", error);
                }
            }

            Ok(())
        }
        ForgeArgs::UploadFile {
            gate,
            path,
            kind,
            url,
            file,
        } => {
            if forge_url.is_none() {
                return Err(Error::NoForgeConnected);
            }
            let forge_url = forge_url.unwrap();
            let gate = Gate::new(gate)?;
            let component = Component::open_local(path)?;

            let gate_id = gate.id.ok_or(Error::GateNoId)?;

            let name = component.recipe.name;
            let version = component
                .recipe
                .version
                .ok_or(Error::ComponentIncomplete(String::from("version")))?;
            let revision = component.recipe.revision.unwrap_or(String::from("0"));

            let response_body: Response<upload_component_file::ResponseData> =
                if let Some(file) = file {
                    let vars = upload_component_file::Variables {
                        name,
                        version,
                        revision,
                        gate: gate_id,
                        url: None,
                        file: Some(Upload(0)),
                        kind: kind.into(),
                    };
                    let request_body = UploadComponentFile::build_query(vars);
                    let request_str = serde_json::to_string(&request_body)?;
                    let client = reqwest::Client::new();

                    let file_map = "{ \"0\": [\"variables.file\"] }";

                    let async_file = tokio::fs::File::open(file).await?;

                    let some_file = reqwest::multipart::Part::stream(async_file);

                    let form = reqwest::multipart::Form::new()
                        .text("operations", request_str)
                        .text("map", file_map)
                        .part("0", some_file);

                    let res = client.post(forge_url).multipart(form).send().await?;
                    res.json().await?
                } else {
                    let vars = upload_component_file::Variables {
                        name,
                        version,
                        revision,
                        gate: gate_id,
                        url: url.clone().map(|u| u.to_string()),
                        file: None,
                        kind: kind.into(),
                    };
                    let client = reqwest::Client::new();
                    let request_body = UploadComponentFile::build_query(vars);

                    let res = client.post(forge_url).json(&request_body).send().await?;
                    res.json().await?
                };

            if let Some(_) = response_body.data {
                println!("File Uploaded");
            } else {
                let errors = response_body.errors.unwrap();
                for error in errors {
                    println!("the server returned errors");
                    println!("{}", error);
                }
            }

            Ok(())
        }
    }
}

pub async fn get_oauth_login_info(
    target_override: Option<Url>,
    host: &str,
    scheme: &str,
) -> Result<AuthConfig, Error> {
    let resp = if let Some(target_override) = target_override {
        let mut target_override = target_override.clone();
        target_override.set_path("/api/v1/auth/login_info");
        reqwest::Client::new()
            .get(target_override)
            .header(reqwest::header::HOST, host)
            .send()
            .await?
    } else {
        reqwest::get(format!("{scheme}://{host}/api/v1/auth/login_info")).await?
    };

    let login_info: AuthConfig = resp.json().await?;
    Ok(login_info)
}
