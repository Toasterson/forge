use crate::storage::jj_backend::create_store_factories;
use crate::storage::seaweedfs::SeaweedFsConfig;
use crate::types::{ComponentId, GateId, RepoId};
use jj_lib::repo::StoreFactories;
use jj_lib::settings::UserSettings;
use jj_lib::workspace::Workspace;
use miette::{Context, IntoDiagnostic, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct JjRepoManager {
    /// Root directory for all repos
    root: PathBuf,
    /// Backend factory for creating new repos
    store_factories: StoreFactories,
    /// SeaweedFS configuration
    seaweedfs_config: SeaweedFsConfig,
    /// User settings for jj operations
    settings: UserSettings,
    /// Cache of loaded repos (workspace per repo)
    repo_cache: Arc<RwLock<HashMap<RepoId, Arc<Workspace>>>>,
}

impl JjRepoManager {
    pub fn new(root: PathBuf, seaweedfs_config: SeaweedFsConfig) -> Result<Self> {
        let store_factories = create_store_factories(seaweedfs_config.clone());

        // Create default user settings
        let config = jj_lib::config::StackedConfig::empty();
        let settings = UserSettings::from_config(config);

        Ok(Self {
            root,
            store_factories,
            seaweedfs_config,
            settings,
            repo_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Ensure a repository exists for a component
    pub async fn ensure_component_repo(&self, component_id: &ComponentId) -> Result<Arc<Workspace>> {
        let repo_path = self.root.join("components").join(&component_id.0);

        if !repo_path.exists() {
            self.init_repo(&repo_path).await?;
        }

        self.load_workspace(&repo_path, RepoId::Component(component_id.clone()))
            .await
    }

    /// Ensure a repository exists for a gate
    pub async fn ensure_gate_repo(&self, gate_id: &GateId) -> Result<Arc<Workspace>> {
        let repo_path = self.root.join("gates").join(&gate_id.0);

        if !repo_path.exists() {
            self.init_repo(&repo_path).await?;
        }

        self.load_workspace(&repo_path, RepoId::Gate(gate_id.clone()))
            .await
    }

    /// Get an already-loaded workspace (doesn't initialize if missing)
    pub async fn get_workspace(&self, repo_id: &RepoId) -> Result<Arc<Workspace>> {
        let cache = self.repo_cache.read().await;
        if let Some(ws) = cache.get(repo_id) {
            return Ok(ws.clone());
        }
        drop(cache);

        // Not in cache, try to load
        let repo_path = match repo_id {
            RepoId::Component(id) => self.root.join("components").join(&id.0),
            RepoId::Gate(id) => self.root.join("gates").join(&id.0),
        };

        if !repo_path.exists() {
            return Err(miette::miette!("repository does not exist: {:?}", repo_path));
        }

        self.load_workspace(&repo_path, repo_id.clone()).await
    }

    /// List all repository IDs
    pub async fn list_all_repos(&self) -> Vec<RepoId> {
        let mut repos = Vec::new();

        // List component repos
        if let Ok(entries) = std::fs::read_dir(self.root.join("components")) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        repos.push(RepoId::Component(ComponentId(name.to_string())));
                    }
                }
            }
        }

        // List gate repos
        if let Ok(entries) = std::fs::read_dir(self.root.join("gates")) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        repos.push(RepoId::Gate(GateId(name.to_string())));
                    }
                }
            }
        }

        repos
    }

    /// Initialize a new repository with our custom backend
    async fn init_repo(&self, repo_path: &std::path::Path) -> Result<()> {
        tokio::fs::create_dir_all(repo_path)
            .await
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to create repo directory: {:?}", repo_path))?;

        // Initialize workspace with custom SeaweedFS backend
        Workspace::init_with_backend(
            &self.settings,
            repo_path,
            &|settings, store_path| {
                let backend = crate::storage::jj_backend::SeaweedFsBackend::init(
                    settings,
                    store_path,
                    self.seaweedfs_config.clone(),
                )
                .map_err(|e| format!("{}", e))?;
                Ok(Box::new(backend))
            },
            jj_lib::signing::Signer::None,
        )
        .into_diagnostic()
        .wrap_err("failed to initialize workspace")?;

        Ok(())
    }

    /// Load a workspace and cache it
    async fn load_workspace(
        &self,
        repo_path: &std::path::Path,
        repo_id: RepoId,
    ) -> Result<Arc<Workspace>> {
        // Check cache first
        {
            let cache = self.repo_cache.read().await;
            if let Some(ws) = cache.get(&repo_id) {
                return Ok(ws.clone());
            }
        }

        // Load workspace
        let ws = Workspace::load(&self.settings, repo_path, &self.store_factories)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to load workspace from {:?}", repo_path))?;

        let ws = Arc::new(ws);

        // Cache it
        {
            let mut cache = self.repo_cache.write().await;
            cache.insert(repo_id, ws.clone());
        }

        Ok(ws)
    }

    /// Reload a workspace (useful after operations that modify it)
    pub async fn reload_workspace(&self, repo_id: &RepoId) -> Result<Arc<Workspace>> {
        // Remove from cache
        {
            let mut cache = self.repo_cache.write().await;
            cache.remove(repo_id);
        }

        // Load fresh
        let repo_path = match repo_id {
            RepoId::Component(id) => self.root.join("components").join(&id.0),
            RepoId::Gate(id) => self.root.join("gates").join(&id.0),
        };

        self.load_workspace(&repo_path, repo_id.clone()).await
    }
}
