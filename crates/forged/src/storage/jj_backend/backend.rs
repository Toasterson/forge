use crate::storage::seaweedfs::{BlobKey, BlobType, SeaweedFsClient, SeaweedFsConfig};
use crate::types::ContentHash;
use async_trait::async_trait;
use jj_lib::backend::{
    Backend, BackendError, BackendResult, ChangeId, Commit, CommitId, Conflict, ConflictId, FileId,
    SymlinkId, Tree, TreeId,
};
use jj_lib::object_id::ObjectId;
use jj_lib::repo_path::RepoPath;
use miette::{Context, IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

use super::serialization::{
    deserialize_commit, deserialize_tree, serialize_commit, serialize_tree,
};

/// Helper to construct `BackendError::Other` from a displayable message.
fn other_err(msg: impl Display) -> BackendError {
    BackendError::Other(msg.to_string().into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendMetadata {
    pub backend_type: String,
    pub version: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct SeaweedFsBackend {
    /// SeaweedFS client for blob operations
    client: Arc<SeaweedFsClient>,
    /// Metadata about this backend instance
    #[allow(dead_code)]
    metadata: BackendMetadata,
}

impl SeaweedFsBackend {
    pub fn init(
        _settings: &jj_lib::settings::UserSettings,
        store_path: &Path,
        config: SeaweedFsConfig,
    ) -> Result<Self> {
        // Initialize backend metadata
        let metadata = BackendMetadata {
            backend_type: "seaweedfs".to_string(),
            version: 1,
            created_at: chrono::Utc::now(),
        };

        // Write metadata to store_path/.jj/repo/store/backend/metadata.json
        let backend_dir = store_path.join(".jj/repo/store/backend");
        std::fs::create_dir_all(&backend_dir)
            .into_diagnostic()
            .wrap_err("failed to create backend directory")?;

        let metadata_path = backend_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .into_diagnostic()
            .wrap_err("failed to serialize metadata")?;
        std::fs::write(&metadata_path, metadata_json)
            .into_diagnostic()
            .wrap_err("failed to write metadata file")?;

        let client = Arc::new(SeaweedFsClient::new(config));

        Ok(Self { client, metadata })
    }

    pub fn load(
        _settings: &jj_lib::settings::UserSettings,
        store_path: &Path,
        config: SeaweedFsConfig,
    ) -> Result<Self> {
        let metadata_path = store_path.join(".jj/repo/store/backend/metadata.json");

        let metadata: BackendMetadata = if metadata_path.exists() {
            let metadata_str = std::fs::read_to_string(&metadata_path)
                .into_diagnostic()
                .wrap_err("failed to read metadata file")?;
            serde_json::from_str(&metadata_str)
                .into_diagnostic()
                .wrap_err("failed to parse metadata")?
        } else {
            // If metadata doesn't exist, create it
            let metadata = BackendMetadata {
                backend_type: "seaweedfs".to_string(),
                version: 1,
                created_at: chrono::Utc::now(),
            };

            let backend_dir = store_path.join(".jj/repo/store/backend");
            std::fs::create_dir_all(&backend_dir)
                .into_diagnostic()
                .wrap_err("failed to create backend directory")?;

            let metadata_json = serde_json::to_string_pretty(&metadata)
                .into_diagnostic()
                .wrap_err("failed to serialize metadata")?;
            std::fs::write(&metadata_path, metadata_json)
                .into_diagnostic()
                .wrap_err("failed to write metadata file")?;

            metadata
        };

        let client = Arc::new(SeaweedFsClient::new(config));

        Ok(Self { client, metadata })
    }

    fn commit_id_to_blob_key(&self, id: &CommitId) -> BlobKey {
        let hash = ContentHash::from_bytes(&id.to_bytes());
        BlobKey::new(hash, BlobType::Commit)
    }

    fn tree_id_to_blob_key(&self, id: &TreeId) -> BlobKey {
        let hash = ContentHash::from_bytes(&id.to_bytes());
        BlobKey::new(hash, BlobType::Tree)
    }

    fn file_id_to_blob_key(&self, id: &FileId) -> BlobKey {
        let hash = ContentHash::from_bytes(&id.to_bytes());
        BlobKey::new(hash, BlobType::File)
    }

    fn symlink_id_to_blob_key(&self, id: &SymlinkId) -> BlobKey {
        let hash = ContentHash::from_bytes(&id.to_bytes());
        BlobKey::new(hash, BlobType::Symlink)
    }

    #[allow(dead_code)]
    fn conflict_id_to_blob_key(&self, id: &ConflictId) -> BlobKey {
        let hash = ContentHash::from_bytes(&id.to_bytes());
        BlobKey::new(hash, BlobType::Conflict)
    }
}

#[async_trait]
impl Backend for SeaweedFsBackend {
    fn name(&self) -> &str {
        "seaweedfs"
    }

    fn commit_id_length(&self) -> usize {
        32 // SHA256 length
    }

    fn change_id_length(&self) -> usize {
        32 // SHA256 length
    }

    fn root_commit_id(&self) -> &CommitId {
        // Standard root commit ID (all zeros)
        static ROOT_COMMIT_ID: std::sync::OnceLock<CommitId> = std::sync::OnceLock::new();
        ROOT_COMMIT_ID.get_or_init(|| CommitId::from_bytes(&[0; 32]))
    }

    fn root_change_id(&self) -> &ChangeId {
        // Standard root change ID (all zeros)
        static ROOT_CHANGE_ID: std::sync::OnceLock<ChangeId> = std::sync::OnceLock::new();
        ROOT_CHANGE_ID.get_or_init(|| ChangeId::from_bytes(&[0; 32]))
    }

    fn empty_tree_id(&self) -> &TreeId {
        // Calculate the ID of an empty tree
        static EMPTY_TREE_ID: std::sync::OnceLock<TreeId> = std::sync::OnceLock::new();
        EMPTY_TREE_ID.get_or_init(|| {
            let empty_tree = Tree::default();
            let serialized = serialize_tree(&empty_tree).expect("failed to serialize empty tree");
            let hash = ContentHash::from_bytes(&serialized);
            TreeId::from_bytes(hash.as_bytes())
        })
    }

    async fn read_commit(&self, id: &CommitId) -> BackendResult<Commit> {
        let blob_key = self.commit_id_to_blob_key(id);

        let cache_path = format!(".seaweedfs_cache/{}.fid", blob_key.hash.hex());

        let fid = std::fs::read_to_string(&cache_path)
            .map_err(|e| other_err(format!("failed to read fid from cache: {}", e)))?;

        let bytes = self
            .client
            .read_blob_by_fid(&fid)
            .await
            .map_err(|e| other_err(format!("failed to read blob: {}", e)))?;

        deserialize_commit(&bytes)
            .map_err(|e| other_err(format!("failed to deserialize commit: {}", e)))
    }

    async fn write_commit(
        &self,
        contents: Commit,
        _sign_with: Option<&mut jj_lib::backend::SigningFn>,
    ) -> BackendResult<(CommitId, Commit)> {
        // 1. Serialize commit
        let bytes = serialize_commit(&contents)
            .map_err(|e| other_err(format!("failed to serialize commit: {}", e)))?;

        // 2. Calculate content hash (SHA256)
        let hash = ContentHash::from_bytes(&bytes);
        let commit_id = CommitId::from_bytes(hash.as_bytes());

        // 3. Write to SeaweedFS using content-addressed key
        let blob_key = self.commit_id_to_blob_key(&commit_id);
        let metadata = self
            .client
            .write_blob(&blob_key, &bytes)
            .await
            .map_err(|e| other_err(format!("failed to write blob: {}", e)))?;

        // 4. Cache the fid locally (in full implementation, store in PostgreSQL)
        let cache_dir = ".seaweedfs_cache";
        std::fs::create_dir_all(cache_dir).ok();
        let cache_path = format!("{}/{}.fid", cache_dir, blob_key.hash.hex());
        std::fs::write(&cache_path, &metadata.fid).ok();

        Ok((commit_id, contents))
    }

    async fn read_tree(&self, _path: &RepoPath, id: &TreeId) -> BackendResult<Tree> {
        let blob_key = self.tree_id_to_blob_key(id);

        let cache_path = format!(".seaweedfs_cache/{}.fid", blob_key.hash.hex());
        let fid = std::fs::read_to_string(&cache_path)
            .map_err(|e| other_err(format!("failed to read fid from cache: {}", e)))?;

        let bytes = self
            .client
            .read_blob_by_fid(&fid)
            .await
            .map_err(|e| other_err(format!("failed to read blob: {}", e)))?;

        deserialize_tree(&bytes)
            .map_err(|e| other_err(format!("failed to deserialize tree: {}", e)))
    }

    async fn write_tree(&self, _path: &RepoPath, contents: &Tree) -> BackendResult<TreeId> {
        let bytes = serialize_tree(contents)
            .map_err(|e| other_err(format!("failed to serialize tree: {}", e)))?;

        let hash = ContentHash::from_bytes(&bytes);
        let tree_id = TreeId::from_bytes(hash.as_bytes());

        let blob_key = self.tree_id_to_blob_key(&tree_id);
        let metadata = self
            .client
            .write_blob(&blob_key, &bytes)
            .await
            .map_err(|e| other_err(format!("failed to write blob: {}", e)))?;

        let cache_dir = ".seaweedfs_cache";
        std::fs::create_dir_all(cache_dir).ok();
        let cache_path = format!("{}/{}.fid", cache_dir, blob_key.hash.hex());
        std::fs::write(&cache_path, &metadata.fid).ok();

        Ok(tree_id)
    }

    async fn read_file(&self, _path: &RepoPath, id: &FileId) -> BackendResult<Box<dyn Read>> {
        let blob_key = self.file_id_to_blob_key(id);

        let cache_path = format!(".seaweedfs_cache/{}.fid", blob_key.hash.hex());
        let fid = std::fs::read_to_string(&cache_path)
            .map_err(|e| other_err(format!("failed to read fid from cache: {}", e)))?;

        let bytes = self
            .client
            .read_blob_by_fid(&fid)
            .await
            .map_err(|e| other_err(format!("failed to read blob: {}", e)))?;

        Ok(Box::new(Cursor::new(bytes)))
    }

    async fn write_file(
        &self,
        _path: &RepoPath,
        contents: &mut (dyn Read + Send),
    ) -> BackendResult<FileId> {
        let mut bytes = Vec::new();
        contents
            .read_to_end(&mut bytes)
            .map_err(|e| other_err(format!("failed to read file: {}", e)))?;

        let hash = ContentHash::from_bytes(&bytes);
        let file_id = FileId::from_bytes(hash.as_bytes());

        let blob_key = self.file_id_to_blob_key(&file_id);
        let metadata = self
            .client
            .write_blob(&blob_key, &bytes)
            .await
            .map_err(|e| other_err(format!("failed to write blob: {}", e)))?;

        let cache_dir = ".seaweedfs_cache";
        std::fs::create_dir_all(cache_dir).ok();
        let cache_path = format!("{}/{}.fid", cache_dir, blob_key.hash.hex());
        std::fs::write(&cache_path, &metadata.fid).ok();

        Ok(file_id)
    }

    async fn read_symlink(&self, _path: &RepoPath, id: &SymlinkId) -> BackendResult<String> {
        let blob_key = self.symlink_id_to_blob_key(id);

        let cache_path = format!(".seaweedfs_cache/{}.fid", blob_key.hash.hex());
        let fid = std::fs::read_to_string(&cache_path)
            .map_err(|e| other_err(format!("failed to read fid from cache: {}", e)))?;

        let bytes = self
            .client
            .read_blob_by_fid(&fid)
            .await
            .map_err(|e| other_err(format!("failed to read blob: {}", e)))?;

        String::from_utf8(bytes).map_err(|e| other_err(format!("invalid UTF-8 in symlink: {}", e)))
    }

    async fn write_symlink(&self, _path: &RepoPath, target: &str) -> BackendResult<SymlinkId> {
        let bytes = target.as_bytes();
        let hash = ContentHash::from_bytes(bytes);
        let symlink_id = SymlinkId::from_bytes(hash.as_bytes());

        let blob_key = self.symlink_id_to_blob_key(&symlink_id);
        let metadata = self
            .client
            .write_blob(&blob_key, bytes)
            .await
            .map_err(|e| other_err(format!("failed to write blob: {}", e)))?;

        let cache_dir = ".seaweedfs_cache";
        std::fs::create_dir_all(cache_dir).ok();
        let cache_path = format!("{}/{}.fid", cache_dir, blob_key.hash.hex());
        std::fs::write(&cache_path, &metadata.fid).ok();

        Ok(symlink_id)
    }

    // read_conflict and write_conflict are SYNC in jj-lib 0.24
    fn read_conflict(&self, _path: &RepoPath, _id: &ConflictId) -> BackendResult<Conflict> {
        // Conflict support is stubbed — return empty conflict
        Ok(Conflict::default())
    }

    fn write_conflict(&self, _path: &RepoPath, contents: &Conflict) -> BackendResult<ConflictId> {
        // Stub: hash the debug representation to produce a deterministic ID
        let bytes = format!("{:?}", contents).into_bytes();
        let hash = ContentHash::from_bytes(&bytes);
        let conflict_id = ConflictId::from_bytes(hash.as_bytes());
        Ok(conflict_id)
    }

    fn gc(
        &self,
        _index: &dyn jj_lib::index::Index,
        _keep_newer: std::time::SystemTime,
    ) -> BackendResult<()> {
        // GC not implemented yet
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn concurrency(&self) -> usize {
        16
    }

    fn get_copy_records(
        &self,
        _paths: Option<&[jj_lib::repo_path::RepoPathBuf]>,
        _from: &CommitId,
        _to: &CommitId,
    ) -> BackendResult<
        std::pin::Pin<
            Box<
                dyn futures::stream::Stream<Item = BackendResult<jj_lib::backend::CopyRecord>>
                    + Send,
            >,
        >,
    > {
        // Copy tracking not implemented yet - return empty stream
        use futures::stream;
        Ok(Box::pin(stream::empty()))
    }
}
