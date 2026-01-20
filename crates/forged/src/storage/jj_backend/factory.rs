use super::backend::SeaweedFsBackend;
use crate::storage::seaweedfs::SeaweedFsConfig;
use jj_lib::repo::StoreFactories;
use jj_lib::settings::UserSettings;
use std::path::Path;

/// Create store factories with the SeaweedFS backend registered
pub fn create_store_factories(seaweedfs_config: SeaweedFsConfig) -> StoreFactories {
    let mut factories = StoreFactories::empty();

    let config = seaweedfs_config.clone();
    factories.add_backend(
        "seaweedfs",
        Box::new(move |settings: &UserSettings, store_path: &Path| {
            let backend = SeaweedFsBackend::load(settings, store_path, config.clone())
                .map_err(|e| format!("failed to load SeaweedFS backend: {}", e))?;
            Ok(Box::new(backend))
        }),
    );

    factories
}

/// Initialize a new repository with the SeaweedFS backend
pub fn init_backend_in_store(
    settings: &UserSettings,
    store_path: &Path,
    config: SeaweedFsConfig,
) -> miette::Result<()> {
    SeaweedFsBackend::init(settings, store_path, config)?;
    Ok(())
}
