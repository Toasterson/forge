use crate::entities::{component_file, source_archive, ComponentFile, SourceArchive};
use crate::repositories::{ApplicationBlobType, BlobRepository};
use crate::services::RbacService;
use miette::{Context, Result};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use std::sync::Arc;

/// Blob download service with access control
/// Verifies blob ownership before allowing downloads
#[derive(Clone)]
pub struct BlobService {
    db: Arc<DatabaseConnection>,
    blob_repo: Arc<BlobRepository>,
    rbac: Arc<RbacService>,
}

impl BlobService {
    pub fn new(
        db: Arc<DatabaseConnection>,
        blob_repo: Arc<BlobRepository>,
        rbac: Arc<RbacService>,
    ) -> Self {
        Self {
            db,
            blob_repo,
            rbac,
        }
    }

    /// Download a blob by hash with access control
    /// Verifies that:
    /// 1. The blob exists and is owned by a component
    /// 2. The actor has ComponentRead permission for that component
    pub async fn download_blob(
        &self,
        actor_id: &str,
        hash: &str,
        blob_type: ApplicationBlobType,
    ) -> Result<Vec<u8>> {
        // 1. Find which component owns this blob
        let component_id = self
            .find_blob_owner(hash, blob_type)
            .await
            .wrap_err("failed to find blob owner")?
            .ok_or_else(|| {
                miette::miette!(
                    "Blob not found or not associated with any component: hash={}. \n\
                     This blob may not have been uploaded yet or was deleted.",
                    hash
                )
            })?;

        // 2. Check if actor has permission to read this component
        if !self
            .rbac
            .check_component_read(actor_id, &component_id)
            .await?
        {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have read access to the component owning this blob. \n\
                 Request access from the gate owner or an administrator.",
                actor_id
            ));
        }

        // 3. Download blob
        let data = self
            .blob_repo
            .get_blob(hash, blob_type)
            .await
            .wrap_err_with(|| format!("failed to download blob: hash={}", hash))?;

        tracing::info!(
            actor_id = %actor_id,
            hash = %hash,
            blob_type = %blob_type,
            size_bytes = data.len(),
            "Blob downloaded"
        );

        Ok(data)
    }

    /// Find which component owns a blob by searching component_files and source_archives
    async fn find_blob_owner(
        &self,
        hash: &str,
        blob_type: ApplicationBlobType,
    ) -> Result<Option<String>> {
        match blob_type {
            ApplicationBlobType::SourceArchive => {
                // Search source_archives table
                let archive = SourceArchive::find()
                    .filter(source_archive::Column::BlobHash.eq(hash))
                    .one(&*self.db)
                    .await
                    .map_err(|e| miette::miette!("database error: {}", e))?;

                Ok(archive.map(|a| a.component_id))
            }
            ApplicationBlobType::Patch
            | ApplicationBlobType::License
            | ApplicationBlobType::Script => {
                // Search component_files table
                let file = ComponentFile::find()
                    .filter(component_file::Column::BlobHash.eq(hash))
                    .filter(component_file::Column::Kind.eq(blob_type.to_string()))
                    .one(&*self.db)
                    .await
                    .map_err(|e| miette::miette!("database error: {}", e))?;

                Ok(file.map(|f| f.component_id))
            }
        }
    }

    /// Stream a blob in chunks (for large files)
    /// Returns an iterator over chunks
    pub async fn download_blob_chunks(
        &self,
        actor_id: &str,
        hash: &str,
        blob_type: ApplicationBlobType,
        chunk_size: usize,
    ) -> Result<Vec<Vec<u8>>> {
        // Download full blob with permission check
        let data = self.download_blob(actor_id, hash, blob_type).await?;

        // Split into chunks
        let chunks: Vec<Vec<u8>> = data
            .chunks(chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        Ok(chunks)
    }

    /// Check if an actor can download a specific blob
    /// Returns true if the actor has permission, false otherwise
    pub async fn can_download_blob(
        &self,
        actor_id: &str,
        hash: &str,
        blob_type: ApplicationBlobType,
    ) -> Result<bool> {
        // Find blob owner
        let component_id = match self.find_blob_owner(hash, blob_type).await? {
            Some(id) => id,
            None => return Ok(false), // Blob doesn't exist
        };

        // Check permission
        self.rbac
            .check_component_read(actor_id, &component_id)
            .await
    }

    /// Get blob metadata (size, component owner) without downloading
    pub async fn get_blob_info(
        &self,
        actor_id: &str,
        hash: &str,
        blob_type: ApplicationBlobType,
    ) -> Result<BlobInfo> {
        // Find blob owner
        let component_id = self
            .find_blob_owner(hash, blob_type)
            .await?
            .ok_or_else(|| miette::miette!("Blob not found: hash={}", hash))?;

        // Check permission
        if !self
            .rbac
            .check_component_read(actor_id, &component_id)
            .await?
        {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have read access to this blob",
                actor_id
            ));
        }

        // Get blob metadata from database
        let metadata = self
            .blob_repo
            .get_blob_metadata_by_hash(hash)
            .await
            .wrap_err("failed to get blob metadata")?
            .ok_or_else(|| miette::miette!("Blob metadata not found"))?;

        Ok(BlobInfo {
            hash: metadata.hash,
            blob_type: metadata.blob_type,
            size_bytes: metadata.size_bytes,
            component_id,
            fid: metadata.fid,
        })
    }
}

/// Blob information
#[derive(Debug, Clone)]
pub struct BlobInfo {
    pub hash: String,
    pub blob_type: String,
    pub size_bytes: i64,
    pub component_id: String,
    pub fid: String,
}
