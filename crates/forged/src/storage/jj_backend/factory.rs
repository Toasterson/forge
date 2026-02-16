use super::backend::SeaweedFsBackend;
use crate::storage::seaweedfs::SeaweedFsConfig;
use jj_lib::backend::BackendLoadError;
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
            let backend =
                SeaweedFsBackend::load(settings, store_path, config.clone()).map_err(|e| {
                    BackendLoadError(format!("failed to load SeaweedFS backend: {}", e).into())
                })?;
            Ok(Box::new(backend))
        }),
    );

    factories
}
