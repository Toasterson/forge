use crate::entities::{source_archive, SourceArchive};
use crate::repositories::{ApplicationBlobType, BlobRepository};
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::sync::Arc;

/// Repository for source archive management
#[derive(Clone)]
pub struct SourceArchiveRepository {
    db: Arc<DatabaseConnection>,
    blob_repo: Arc<BlobRepository>,
}

impl SourceArchiveRepository {
    pub fn new(db: Arc<DatabaseConnection>, blob_repo: Arc<BlobRepository>) -> Self {
        Self { db, blob_repo }
    }

    /// Add a source archive to a component
    /// Stores the blob in SeaweedFS and creates metadata record
    pub async fn add_source_archive(
        &self,
        component_id: &str,
        filename: String,
        url: Option<String>,
        data: &[u8],
    ) -> Result<source_archive::Model> {
        // 1. Store blob via BlobRepository
        let (blob_hash, _fid) = self
            .blob_repo
            .store_blob(data, ApplicationBlobType::SourceArchive)
            .await
            .wrap_err("failed to store source archive blob")?;

        // 2. Create metadata record
        let model = source_archive::ActiveModel {
            component_id: Set(component_id.to_string()),
            filename: Set(filename.clone()),
            url: Set(url.clone()),
            blob_hash: Set(blob_hash.clone()),
            size_bytes: Set(data.len() as i64),
            ..Default::default()
        };

        let archive = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert source archive record")?;

        tracing::info!(
            component_id = %component_id,
            filename = %filename,
            blob_hash = %blob_hash,
            size_bytes = data.len(),
            "Added source archive"
        );

        Ok(archive)
    }

    /// List all source archives for a component
    pub async fn list_for_component(
        &self,
        component_id: &str,
    ) -> Result<Vec<source_archive::Model>> {
        let archives = SourceArchive::find()
            .filter(source_archive::Column::ComponentId.eq(component_id))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list source archives")?;

        Ok(archives)
    }

    /// Get a specific source archive by ID
    pub async fn get_archive(&self, archive_id: i64) -> Result<Option<source_archive::Model>> {
        let archive = SourceArchive::find_by_id(archive_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get source archive")?;

        Ok(archive)
    }

    /// Get the blob data for a source archive
    pub async fn get_archive_data(&self, archive_id: i64) -> Result<Vec<u8>> {
        // 1. Get archive metadata
        let archive = self
            .get_archive(archive_id)
            .await?
            .ok_or_else(|| {
                miette::miette!(
                    "Source archive not found: id={}. \n\
                     This archive may have been deleted or does not exist. \n\
                     Use 'forged component list-archives <component_id>' to see available archives.",
                    archive_id
                )
            })?;

        // 2. Fetch blob data
        let data = self
            .blob_repo
            .get_blob(&archive.blob_hash, ApplicationBlobType::SourceArchive)
            .await
            .wrap_err_with(|| {
                format!(
                    "failed to get archive data: archive_id={}, hash={}",
                    archive_id, archive.blob_hash
                )
            })?;

        Ok(data)
    }

    /// Get archive data by component ID and filename
    pub async fn get_archive_data_by_name(
        &self,
        component_id: &str,
        filename: &str,
    ) -> Result<Vec<u8>> {
        // 1. Find archive by component_id and filename
        let archive = SourceArchive::find()
            .filter(source_archive::Column::ComponentId.eq(component_id))
            .filter(source_archive::Column::Filename.eq(filename))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to find source archive by name")?
            .ok_or_else(|| {
                miette::miette!(
                    "Source archive not found: component_id={}, filename={}. \n\
                     This archive may not have been uploaded yet. \n\
                     Use 'forged component add-archive <component_id> {}' to upload it.",
                    component_id,
                    filename,
                    filename
                )
            })?;

        // 2. Fetch blob data
        let data = self
            .blob_repo
            .get_blob(&archive.blob_hash, ApplicationBlobType::SourceArchive)
            .await
            .wrap_err_with(|| {
                format!(
                    "failed to get archive data: component_id={}, filename={}, hash={}",
                    component_id, filename, archive.blob_hash
                )
            })?;

        Ok(data)
    }

    /// Delete a source archive
    pub async fn delete_archive(&self, archive_id: i64) -> Result<()> {
        let archive = self
            .get_archive(archive_id)
            .await?
            .ok_or_else(|| miette::miette!("Source archive not found: id={}", archive_id))?;

        SourceArchive::delete_by_id(archive_id)
            .exec(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to delete source archive")?;

        tracing::info!(
            archive_id = archive_id,
            component_id = %archive.component_id,
            filename = %archive.filename,
            "Deleted source archive"
        );

        Ok(())
    }

    /// Check if a source archive exists
    pub async fn archive_exists(&self, component_id: &str, filename: &str) -> Result<bool> {
        let exists = SourceArchive::find()
            .filter(source_archive::Column::ComponentId.eq(component_id))
            .filter(source_archive::Column::Filename.eq(filename))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to check archive existence")?
            .is_some();

        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would go here
    // They require a database connection and SeaweedFS instance
}
