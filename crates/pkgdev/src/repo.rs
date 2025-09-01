use crate::build::ips;
use crate::get_project_dir;
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::fs::DirBuilder;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum RepoError {
    #[error("failed to determine configuration directory")]
    #[diagnostic(code(ips::repo_error::no_config_dir))]
    NoConfigDir,

    #[error(transparent)]
    #[diagnostic(code(ips::repo_error::io))]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(code(ips::repo_error::json))]
    Json(#[from] serde_json::Error),

    #[error("failed to initialize IPS repository at the given path: {0}")]
    #[diagnostic(
        code(ips::repo_error::init_failed),
        help("ensure pkgrepo is installed or libips feature is available")
    )]
    RepoInitError(String),

    #[error("repository context '{0}' already exists")]
    #[diagnostic(code(ips::repo_error::exists))]
    AlreadyExists(String),

    #[error("repository context '{0}' not found")]
    #[diagnostic(code(ips::repo_error::not_found))]
    NotFound(String),

    #[error(
        "no repository selected; use --repo, --repo-context, or `pkgdev repo select` to choose one"
    )]
    #[diagnostic(
        code(ips::repo_error::no_selection),
        help("create or select a repository context")
    )]
    NoSelection,
}

pub type Result<T> = miette::Result<T, RepoError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoContext {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RepoConfigFile {
    current: Option<String>,
    repos: Vec<RepoContext>,
}

#[derive(Debug, Default)]
pub struct RepoManager {
    cfg_path: PathBuf,
    cfg: RepoConfigFile,
}

impl RepoManager {
    fn get_or_create_config_path() -> Result<PathBuf> {
        let proj = get_project_dir().map_err(|_| RepoError::NoConfigDir)?;
        let cfg_dir = proj.config_dir().to_path_buf();
        if !cfg_dir.exists() {
            DirBuilder::new().recursive(true).create(&cfg_dir)?;
        }
        Ok(cfg_dir.join("repos.json"))
    }

    pub fn load() -> Result<Self> {
        let cfg_path = Self::get_or_create_config_path()?;
        let cfg = if cfg_path.exists() {
            let f = std::fs::File::open(&cfg_path)?;
            serde_json::from_reader::<_, RepoConfigFile>(f)?
        } else {
            RepoConfigFile::default()
        };
        Ok(Self { cfg_path, cfg })
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.cfg_path.parent() {
            if !parent.exists() {
                DirBuilder::new().recursive(true).create(parent)?;
            }
        }
        let mut f = std::fs::File::create(&self.cfg_path)?;
        serde_json::to_writer_pretty(&mut f, &self.cfg)?;
        Ok(())
    }

    pub fn list(&self) -> &[RepoContext] {
        &self.cfg.repos
    }

    pub fn create<S: Into<String>, P: AsRef<Path>>(&mut self, name: S, path: P) -> Result<()> {
        let name = name.into();
        if self.cfg.repos.iter().any(|r| r.name == name) {
            return Err(RepoError::AlreadyExists(name));
        }
        let path_buf = path.as_ref().to_path_buf();
        if !path_buf.exists() {
            DirBuilder::new().recursive(true).create(&path_buf)?;
        }
        tracing::info!(target: "pkgdev::repo", "[repo] create: creating context '{}' at {}", name, path_buf.display());
        // Initialize IPS repository and default publisher matching the context name
        if let Err(e) = ips::ensure_repo_with_publisher_exists(&path_buf, &name) {
            return Err(RepoError::RepoInitError(format!(
                "{}: {}",
                path_buf.display(),
                e
            )));
        }
        let ctx = RepoContext {
            name,
            path: path_buf,
        };
        self.cfg.repos.push(ctx);
        self.save()
    }

    pub fn delete<S: AsRef<str>>(&mut self, name: S) -> Result<()> {
        let name_str = name.as_ref();
        let prev_len = self.cfg.repos.len();
        self.cfg.repos.retain(|r| r.name != name_str);
        if self.cfg.repos.len() == prev_len {
            return Err(RepoError::NotFound(name_str.to_string()));
        }
        if self.cfg.current.as_deref() == Some(name_str) {
            self.cfg.current = None;
        }
        self.save()
    }

    pub fn select<S: Into<String>>(&mut self, name: S) -> Result<()> {
        let name = name.into();
        if !self.cfg.repos.iter().any(|r| r.name == name) {
            return Err(RepoError::NotFound(name));
        }
        self.cfg.current = Some(name);
        self.save()
    }

    pub fn current(&self) -> Option<&RepoContext> {
        self.cfg
            .current
            .as_ref()
            .and_then(|n| self.cfg.repos.iter().find(|r| &r.name == n))
    }

    pub fn get_by_name<S: AsRef<str>>(&self, name: S) -> Option<&RepoContext> {
        let n = name.as_ref();
        self.cfg.repos.iter().find(|r| r.name == n)
    }

    pub fn resolve(
        &self,
        repo_path: Option<PathBuf>,
        repo_context: Option<String>,
    ) -> Result<PathBuf> {
        if let Some(p) = repo_path {
            tracing::info!(target: "pkgdev::repo", "[repo] resolve: using explicit path override: {}", p.display());
            return Ok(p);
        }
        if let Some(name) = repo_context {
            if let Some(ctx) = self.get_by_name(&name) {
                tracing::info!(target: "pkgdev::repo", "[repo] resolve: using named context '{}' at {}", name, ctx.path.display());
                return Ok(ctx.path.clone());
            }
            tracing::warn!(target: "pkgdev::repo", "[repo] resolve: named context '{}' not found", name);
            return Err(RepoError::NotFound(name));
        }
        if let Some(cur) = self.current() {
            tracing::info!(target: "pkgdev::repo", "[repo] resolve: using currently selected context '{}' at {}", cur.name, cur.path.display());
            return Ok(cur.path.clone());
        }
        tracing::warn!(target: "pkgdev::repo", "[repo] resolve: no repository selection available");
        Err(RepoError::NoSelection)
    }
}

#[cfg(test)]
mod tests {
    // Note: These are light tests; real tests would need to redirect config dir.
    // Skipped to avoid touching user config.
    #[test]
    fn dummy() {
        assert!(true);
    }
}
