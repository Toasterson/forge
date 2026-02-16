use crate::storage::jj_backend::create_store_factories;
use crate::storage::seaweedfs::SeaweedFsConfig;
use crate::types::{ComponentId, GateId, RepoId};
use miette::{Context, IntoDiagnostic, Result};
use std::path::PathBuf;

/// Manages Jujutsu repositories on disk.
///
/// All jj-lib types (`StoreFactories`, `Workspace`, `UserSettings`) are
/// **not** `Send+Sync`, so they cannot be stored in a struct shared across
/// async tasks.  Instead we keep only `Send+Sync` data here and create the
/// jj-lib objects on demand inside `spawn_blocking` closures.
pub struct JjRepoManager {
    /// Root directory for all repos
    root: PathBuf,
    /// SeaweedFS configuration (cloned into blocking closures)
    seaweedfs_config: SeaweedFsConfig,
}

impl JjRepoManager {
    pub fn new(root: PathBuf, seaweedfs_config: SeaweedFsConfig) -> Result<Self> {
        // Ensure root directory exists
        std::fs::create_dir_all(&root)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to create jj repo root: {:?}", root))?;

        Ok(Self {
            root,
            seaweedfs_config,
        })
    }

    /// Resolve a `RepoId` to its on-disk path.
    fn repo_path(&self, repo_id: &RepoId) -> PathBuf {
        match repo_id {
            RepoId::Gate(id) => self.root.join("gates").join(&id.0),
            RepoId::Component(id) => self.root.join("components").join(&id.0),
        }
    }

    /// Ensure the repository exists (init if missing), then write the given
    /// files and commit them in a single jj transaction.
    ///
    /// `files` is a list of `(relative_path, content)` pairs.
    pub async fn ensure_and_commit(
        &self,
        repo_id: &RepoId,
        files: Vec<(String, Vec<u8>)>,
        message: String,
    ) -> Result<()> {
        let repo_path = self.repo_path(repo_id);
        let config = self.seaweedfs_config.clone();

        tokio::task::spawn_blocking(move || {
            Self::blocking_ensure_and_commit(&repo_path, config, files, message)
        })
        .await
        .into_diagnostic()
        .wrap_err("jj task panicked")?
    }

    /// Synchronous core: init if needed, write files, commit.
    fn blocking_ensure_and_commit(
        repo_path: &std::path::Path,
        config: SeaweedFsConfig,
        files: Vec<(String, Vec<u8>)>,
        message: String,
    ) -> Result<()> {
        let jj_config = jj_lib::config::StackedConfig::empty();
        let settings = jj_lib::settings::UserSettings::from_config(jj_config);
        let store_factories = create_store_factories(config.clone());
        let wc_factories = jj_lib::workspace::WorkingCopyFactories::default();

        // Init if the repo doesn't exist yet
        if !repo_path.join(".jj").exists() {
            std::fs::create_dir_all(repo_path)
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to create repo directory: {:?}", repo_path))?;

            let config_for_init = config;
            jj_lib::workspace::Workspace::init_with_backend(
                &settings,
                repo_path,
                &|init_settings, store_path| {
                    let backend = crate::storage::jj_backend::SeaweedFsBackend::init(
                        init_settings,
                        store_path,
                        config_for_init.clone(),
                    )
                    .map_err(|e| {
                        jj_lib::backend::BackendInitError(
                            format!("init SeaweedFS backend: {}", e).into(),
                        )
                    })?;
                    Ok(Box::new(backend))
                },
                jj_lib::signing::Signer::default(),
            )
            .into_diagnostic()
            .wrap_err("failed to initialize jj workspace")?;
        }

        // Load workspace and start a transaction
        let ws = jj_lib::workspace::Workspace::load(
            &settings,
            repo_path,
            &store_factories,
            &wc_factories,
        )
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to load jj workspace from {:?}", repo_path))?;

        let repo = ws
            .repo_loader()
            .load_at_head(&settings)
            .into_diagnostic()
            .wrap_err("failed to load repo at head")?;

        let tx = repo.start_transaction(&settings);

        // Write files to the workspace working copy directory
        let ws_root = ws.workspace_root().to_path_buf();
        for (rel_path, content) in &files {
            let full_path = ws_root.join(rel_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to create directory: {:?}", parent))?;
            }
            std::fs::write(&full_path, content)
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write file: {:?}", full_path))?;
        }

        tx.commit(message)
            .into_diagnostic()
            .wrap_err("failed to commit jj transaction")?;

        Ok(())
    }

    /// Initialize a repo without writing any files (useful for pre-creation).
    pub async fn init_repo(&self, repo_id: &RepoId) -> Result<()> {
        let repo_path = self.repo_path(repo_id);
        let config = self.seaweedfs_config.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let jj_config = jj_lib::config::StackedConfig::empty();
            let settings = jj_lib::settings::UserSettings::from_config(jj_config);

            std::fs::create_dir_all(&repo_path)
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to create repo directory: {:?}", repo_path))?;

            jj_lib::workspace::Workspace::init_with_backend(
                &settings,
                &repo_path,
                &|init_settings, store_path| {
                    let backend = crate::storage::jj_backend::SeaweedFsBackend::init(
                        init_settings,
                        store_path,
                        config.clone(),
                    )
                    .map_err(|e| {
                        jj_lib::backend::BackendInitError(
                            format!("init SeaweedFS backend: {}", e).into(),
                        )
                    })?;
                    Ok(Box::new(backend))
                },
                jj_lib::signing::Signer::default(),
            )
            .into_diagnostic()
            .wrap_err("failed to initialize jj workspace")?;

            Ok(())
        })
        .await
        .into_diagnostic()
        .wrap_err("jj init task panicked")?
    }

    /// List all repository IDs by scanning the filesystem.
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
}
