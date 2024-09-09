use miette::Diagnostic;
use std::fs::DirBuilder;
use std::path::{Path, PathBuf};
use config::{Value, ValueKind};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use workspace::{Workspace, WorkspaceConfig, WorkspaceError};

#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum ConfigError {
    #[error(transparent)]
    ConfigError(#[from] config::ConfigError),

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error("no project directory could be derived. Is this cli running on a supported OS?")]
    NoProjectDir,

    #[error(transparent)]
    WorkspaceError(#[from] WorkspaceError),

    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, ConfigError>;

const QUALIFIER: &str = "org";
const ORG: &str = "openindiana";
const APP_NAME: &str = "pkgdev";
const DEFAULT_WORKSPACE_DIR: &str = "wks";
const DEFAULT_OUTPUT_DIR_DIR: &str = "output";
const DEFAULT_REPO_DIR_DIR: &str = "repo";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    workspace_config: Option<WorkspaceConfig>,
    base_path: Option<String>,
    output_dir: Option<String>,
    pub github_token: Option<GitHubToken>,
    search_path: Option<Vec<String>>,
    pub forges: Vec<ForgeToken>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ForgeToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub scope: Option<Vec<String>>,
    pub expires_in: Option<u64>,
}

impl Into<ValueKind> for ForgeToken {
    fn into(self) -> ValueKind {
        ValueKind::Table(config::Map::from([
            ("access_token".to_string(), Value::from(self.access_token)),
            ("refresh_token".to_string(), Value::from(self.refresh_token)),
            ("scope".to_string(), Value::from(self.scope)),
            ("expires_in".to_string(), Value::from(self.expires_in)),
        ]))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub scope: Option<Vec<String>>,
    pub expires_in: Option<u64>,
}

impl Settings {
    pub fn open() -> Result<Self> {
        let config_dir = Settings::get_or_create_config_dir()?;
        let config = config::Config::builder()
            .set_default("base_path", ".")?
            .set_default(
                "search_path",
                Some(vec![
                    "/usr/gnu/bin",
                    "/usr/bin",
                    "/usr/sbin",
                    "/sbin",
                ]),
            )?
            .set_default("forges", Vec::<ForgeToken>::new())?
            .add_source(config::File::from(config_dir.join("config")).required(false))
            .build()?;

        Ok(config.try_deserialize()?)
    }

    fn get_or_create_config_dir() -> Result<PathBuf> {
        let proj_dir = ProjectDirs::from(QUALIFIER, ORG, APP_NAME).ok_or(ConfigError::NoProjectDir)?;
        let config_dir = proj_dir.config_dir();
        if !config_dir.exists() {
            DirBuilder::new().recursive(true).create(config_dir)?;
        }
        Ok(config_dir.to_path_buf())
    }

    pub fn save(&self) -> Result<()> {
        let config_dir = Settings::get_or_create_config_dir()?;
        let mut file = std::fs::File::create(config_dir.join("config.json"))?;
        serde_json::to_writer(&mut file, &self)?;
        Ok(())
    }

    fn get_or_create_data_dir() -> Result<PathBuf> {
        let proj_dir = ProjectDirs::from(QUALIFIER, ORG, APP_NAME).ok_or(ConfigError::NoProjectDir)?;
        let data_dir = proj_dir.data_dir();
        if !data_dir.exists() {
            DirBuilder::new().recursive(true).create(data_dir)?;
        }
        Ok(data_dir.to_path_buf())
    }

    pub fn get_or_create_archives_dir() -> Result<PathBuf> {
        let proj_dir = ProjectDirs::from(QUALIFIER, ORG, APP_NAME).ok_or(ConfigError::NoProjectDir)?;
        let archive_dir = proj_dir.cache_dir().join("archives");
        if !archive_dir.exists() {
            DirBuilder::new().recursive(true).create(&archive_dir)?;
        }
        Ok(archive_dir.to_path_buf())
    }

    pub fn get_or_create_output_dir() -> Result<PathBuf> {
        let proj_dir = ProjectDirs::from(QUALIFIER, ORG, APP_NAME).ok_or(ConfigError::NoProjectDir)?;
        let output_dir = proj_dir.data_dir().join(DEFAULT_OUTPUT_DIR_DIR);
        if !output_dir.exists() {
            DirBuilder::new().recursive(true).create(&output_dir)?;
        }
        Ok(output_dir.to_path_buf())
    }

    pub fn get_or_create_repo_dir() -> Result<PathBuf> {
        let proj_dir = ProjectDirs::from(QUALIFIER, ORG, APP_NAME).ok_or(ConfigError::NoProjectDir)?;
        let repo_dir = proj_dir.data_dir().join(DEFAULT_REPO_DIR_DIR);
        if !repo_dir.exists() {
            DirBuilder::new().recursive(true).create(&repo_dir)?;
        }
        Ok(repo_dir.to_path_buf())
    }

    pub fn get_output_dir(&self) -> String {
        match &self.output_dir {
            Some(x) => x.to_string(),
            None => DEFAULT_OUTPUT_DIR_DIR.to_owned(),
        }
    }

    pub fn get_search_path(&self) -> Vec<String> {
        match &self.search_path {
            Some(x) => x.to_vec(),
            None => vec![
                "/usr/gnu/bin".into(),
                "/usr/bin".into(),
                "/usr/sbin".into(),
                "/sbin".into(),
            ],
        }
    }

    pub fn add_path_to_search(&mut self, value: String) {
        if let Some(path) = &mut self.search_path {
            path.push(value);
        } else {
            self.search_path = Some(vec![value]);
        };
    }

    pub fn remove_path_from_search(&mut self, value: String) {
        if let Some(path) = &mut self.search_path {
            self.search_path = Some(path.clone().into_iter().filter(|e| e != &value).collect());
        } else {
            self.search_path = Some(vec![value]);
        };
    }

    pub fn get_workspace_from<P: AsRef<Path>>(&self, name: P) -> Result<Workspace> {
        let base_path = if let Some(base_path) = &self.base_path {
            Path::new(base_path).to_path_buf()
        } else {
            Self::get_or_create_data_dir()?
        };

        let wks = Workspace::new(base_path.join(name))?;

        Ok(wks)
    }

    pub fn get_current_wks(&self) -> Result<Workspace> {
        let wks_config = if let Some(current) = &self.workspace_config {
            current.clone()
        } else {
            let base_path = if let Some(base_path) = &self.base_path {
                Path::new(base_path).to_path_buf()
            } else {
                Self::get_or_create_data_dir()?
            };

            WorkspaceConfig {
                path: PathBuf::from(base_path).join(DEFAULT_WORKSPACE_DIR)
            }
        };

        Ok(Workspace::from_config(&wks_config)?)
    }

    pub fn list_workspaces() -> Result<Vec<String>> {
        let data_dir = Self::get_or_create_data_dir()?;
        let workspaces = std::fs::read_dir(&data_dir)?
            .into_iter()
            .map(|e| {
                e.unwrap()
                    .path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<String>>();
        if workspaces.len() == 0 {
            Ok(vec![String::from(DEFAULT_WORKSPACE_DIR)])
        } else {
            Ok(workspaces)
        }
    }

    #[allow(dead_code)]
    pub fn get_or_create_cache_dir() -> Result<PathBuf> {
        let proj_dir = ProjectDirs::from(QUALIFIER, ORG, APP_NAME).ok_or(ConfigError::NoProjectDir)?;
        let cache_dir = proj_dir.cache_dir();
        if !cache_dir.exists() {
            DirBuilder::new().recursive(true).create(cache_dir)?;
        }
        Ok(cache_dir.to_path_buf())
    }

    pub fn change_current_workspace<P: AsRef<Path>>(&mut self, base_path: P, name: Option<String>) -> Result<Workspace> {
        let wks_path = base_path.as_ref().join(name.unwrap_or_default());
        if !wks_path.exists() {
            DirBuilder::new().recursive(true).create(&wks_path)?;
        }

        let wks = WorkspaceConfig{
            path: wks_path,
        };
        self.workspace_config = Some(wks.clone());
        self.save()?;

        Ok(Workspace::from_config(&wks)?)
    }
}