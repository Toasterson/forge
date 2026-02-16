use crate::entities::{blob_metadata, BlobMetadata};
use crate::storage::seaweedfs::client::{BlobKey, SeaweedFsClient};
use crate::types::ContentHash;
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::sync::Arc;

/// Repository for low-level blob operations integrating SeaweedFS + PostgreSQL
#[derive(Clone)]
pub struct BlobRepository {
    db: Arc<DatabaseConnection>,
    seaweedfs: Arc<SeaweedFsClient>,
    namespace: String,
}

impl BlobRepository {
    pub fn new(
        db: Arc<DatabaseConnection>,
        seaweedfs: Arc<SeaweedFsClient>,
        namespace: String,
    ) -> Self {
        Self {
            db,
            seaweedfs,
            namespace,
        }
    }

    /// Store a blob with content-addressed key
    /// Returns (hash, fid) tuple
    /// Handles deduplication - if blob already exists, returns existing metadata
    pub async fn store_blob(
        &self,
        data: &[u8],
        blob_type: ApplicationBlobType,
    ) -> Result<(String, String)> {
        // 1. Compute content hash
        let content_hash = ContentHash::from_bytes(data);
        let hash_hex = content_hash.hex();

        // 2. Check if blob already exists (deduplication)
        if let Some(existing) = self.get_blob_metadata(&hash_hex, blob_type).await? {
            tracing::debug!(
                hash = %hash_hex,
                blob_type = %blob_type,
                "Blob already exists, skipping upload"
            );
            return Ok((existing.hash, existing.fid));
        }

        // 3. Upload to SeaweedFS
        let jj_blob_type = blob_type.to_jj_blob_type();
        let blob_key = BlobKey::new(content_hash.clone(), jj_blob_type);
        let metadata = self
            .seaweedfs
            .write_blob(&blob_key, data)
            .await
            .wrap_err("failed to write blob to SeaweedFS")?;

        // 4. Store metadata in PostgreSQL
        let model = blob_metadata::ActiveModel {
            hash: Set(hash_hex.clone()),
            blob_type: Set(blob_type.to_string()),
            namespace: Set(self.namespace.clone()),
            fid: Set(metadata.fid.clone()),
            size_bytes: Set(data.len() as i64),
            ..Default::default()
        };

        model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert blob metadata into database")?;

        tracing::info!(
            hash = %hash_hex,
            blob_type = %blob_type,
            size_bytes = data.len(),
            fid = %metadata.fid,
            "Stored new blob"
        );

        Ok((hash_hex, metadata.fid))
    }

    /// Get blob data by hash and type
    /// Looks up FID from PostgreSQL and fetches from SeaweedFS
    pub async fn get_blob(&self, hash: &str, blob_type: ApplicationBlobType) -> Result<Vec<u8>> {
        // 1. Look up metadata from PostgreSQL
        let metadata = self
            .get_blob_metadata(hash, blob_type)
            .await?
            .ok_or_else(|| {
                miette::miette!(
                    "Blob not found: hash={}, type={}. \n\
                     This blob may not have been uploaded yet. \n\
                     Use the appropriate upload command to add it first.",
                    hash,
                    blob_type
                )
            })?;

        // 2. Fetch from SeaweedFS using FID
        let data = self
            .seaweedfs
            .read_blob_by_fid(&metadata.fid)
            .await
            .wrap_err_with(|| {
                format!("failed to read blob from SeaweedFS: fid={}", metadata.fid)
            })?;

        tracing::debug!(
            hash = %hash,
            blob_type = %blob_type,
            size_bytes = data.len(),
            "Retrieved blob"
        );

        Ok(data)
    }

    /// Check if a blob exists
    pub async fn blob_exists(&self, hash: &str, blob_type: ApplicationBlobType) -> Result<bool> {
        let exists = BlobMetadata::find()
            .filter(blob_metadata::Column::Hash.eq(hash))
            .filter(blob_metadata::Column::BlobType.eq(blob_type.to_string()))
            .filter(blob_metadata::Column::Namespace.eq(&self.namespace))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to check blob existence")?
            .is_some();

        Ok(exists)
    }

    /// Get blob metadata from PostgreSQL
    async fn get_blob_metadata(
        &self,
        hash: &str,
        blob_type: ApplicationBlobType,
    ) -> Result<Option<blob_metadata::Model>> {
        let metadata = BlobMetadata::find()
            .filter(blob_metadata::Column::Hash.eq(hash))
            .filter(blob_metadata::Column::BlobType.eq(blob_type.to_string()))
            .filter(blob_metadata::Column::Namespace.eq(&self.namespace))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to query blob metadata")?;

        Ok(metadata)
    }

    /// List all blobs of a given type
    pub async fn list_blobs(
        &self,
        blob_type: ApplicationBlobType,
    ) -> Result<Vec<blob_metadata::Model>> {
        let blobs = BlobMetadata::find()
            .filter(blob_metadata::Column::BlobType.eq(blob_type.to_string()))
            .filter(blob_metadata::Column::Namespace.eq(&self.namespace))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list blobs")?;

        Ok(blobs)
    }

    /// Get blob metadata by hash only (for ownership checks)
    pub async fn get_blob_metadata_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<blob_metadata::Model>> {
        let metadata = BlobMetadata::find()
            .filter(blob_metadata::Column::Hash.eq(hash))
            .filter(blob_metadata::Column::Namespace.eq(&self.namespace))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to query blob metadata by hash")?;

        Ok(metadata)
    }
}

/// Application-level blob types (distinct from Jujutsu VCS blob types)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationBlobType {
    SourceArchive,
    Patch,
    License,
    Script,
}

impl ApplicationBlobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApplicationBlobType::SourceArchive => "source_archive",
            ApplicationBlobType::Patch => "patch",
            ApplicationBlobType::License => "license",
            ApplicationBlobType::Script => "script",
        }
    }

    /// Convert to Jujutsu blob type for SeaweedFS storage
    /// All application blobs are stored as "File" type in the Jujutsu backend
    fn to_jj_blob_type(&self) -> crate::storage::seaweedfs::client::BlobType {
        crate::storage::seaweedfs::client::BlobType::File
    }
}

impl std::fmt::Display for ApplicationBlobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ApplicationBlobType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "source_archive" => Ok(ApplicationBlobType::SourceArchive),
            "patch" => Ok(ApplicationBlobType::Patch),
            "license" => Ok(ApplicationBlobType::License),
            "script" => Ok(ApplicationBlobType::Script),
            _ => Err(format!("invalid application blob type: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_type_conversion() {
        assert_eq!(
            ApplicationBlobType::SourceArchive.as_str(),
            "source_archive"
        );
        assert_eq!(ApplicationBlobType::Patch.as_str(), "patch");
        assert_eq!(ApplicationBlobType::License.as_str(), "license");
        assert_eq!(ApplicationBlobType::Script.as_str(), "script");
    }

    #[test]
    fn test_blob_type_from_str() {
        use std::str::FromStr;
        assert_eq!(
            ApplicationBlobType::from_str("source_archive").unwrap(),
            ApplicationBlobType::SourceArchive
        );
        assert_eq!(
            ApplicationBlobType::from_str("patch").unwrap(),
            ApplicationBlobType::Patch
        );
        assert!(ApplicationBlobType::from_str("invalid").is_err());
    }
}
