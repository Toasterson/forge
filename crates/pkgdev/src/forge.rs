use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use clap::Subcommand;
use graphql_client::{GraphQLQuery, Response};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{ParseError, Url};
use gate::{Gate, GateError};
use crate::get_project_dir;

#[derive(Debug, Subcommand)]
pub enum ForgeArgs {
    Connect {
        target: Url,
        #[arg(short, long)]
        select: bool,
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
}

type Result<T, E = Error> = miette::Result<T, E>;

#[derive(Serialize, Deserialize)]
pub struct ForgeConfig {
    pub forges: HashMap<String, Url>,
    pub selected_forge: Option<String>
}

impl ForgeConfig {
    pub fn get_selected(&self) -> Option<Url> {
        if self.forges.len() == 0 {
            None
        } else {
            if let Some(idx) = &self.selected_forge {
                self.forges.get(idx).map(|u| u.clone())
            } else {
                self.forges.clone().into_iter().collect::<Vec<(String,Url)>>().first().map(|(_,u)|u.clone())
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
        ForgeConfig{
            forges: HashMap::new(),
            selected_forge: None,
        }
    };
    Ok(forge_config)
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "forge.graphql.schema.json",
    query_path = "queries/defineGate.graphql",
    response_derives = "Debug,Serialize,PartialEq",
)]
pub struct DefineGateMutation;

pub fn handle_forge_interaction(args: &ForgeArgs) -> Result<()> {
    let mut forge_config = get_forge_config()?;

    match args {
        ForgeArgs::DefineGate { file, name, version, branch, publisher } => {
            let forge_url = forge_config.get_selected();
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

            let (name, version, branch, publisher, transforms) = if name.is_some() && version.is_some() && branch.is_some() && publisher.is_some() {
                // We define everything on the commandline and thus use those variables.
                (name.clone().unwrap(), version.clone().unwrap(), branch.clone().unwrap(), publisher.clone().unwrap(), None)
            } else if let Some(gate) = gate {
                // take the variables from the file
                (gate.name, gate.version, gate.branch, gate.publisher, Some(gate.default_transforms))
            } else {
                // All other cases are user error
                return Err(Error::MissingParameter(String::from("name")))
            };
            
            let variables: define_gate_mutation::Variables = define_gate_mutation::Variables{
                name,
                version,
                branch,
                publisher,
                transforms: transforms.unwrap_or(vec![]).into_iter().map(|t| t.to_string()).collect(),
            };

            let request_body = DefineGateMutation::build_query(variables);
            let client = reqwest::blocking::Client::new();
            let res = client.post(forge_url).json(&request_body).send()?;
            let response_body: Response<define_gate_mutation::ResponseData> = res.json()?;
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
        ForgeArgs::Connect { target, select } => {
            let host = target.host_str();
            if host.is_none() {
                return Err(Error::MissingParameter(String::from("no host in forge URL")))
            }
            
            forge_config.forges.insert(host.unwrap().to_owned(), target.clone());
            
            if *select {
                forge_config.selected_forge = Some(host.unwrap().to_owned());
            }

            let project_dirs = get_project_dir()?;
            let config_dir = project_dirs.config_dir();
            if !config_dir.exists() {
                std::fs::DirBuilder::new().recursive(true).create(config_dir)?;
            }
            let mut f = File::create(config_dir.join("forge.json"))?;
            serde_json::to_writer_pretty(&mut f, &forge_config)?;
            
            Ok(())
        }
    }
}