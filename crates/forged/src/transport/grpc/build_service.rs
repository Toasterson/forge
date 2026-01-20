use super::proto::{
    build_service_server::BuildService, BuildManifest, ComponentFileInfo, ContentHash,
    DownloadBlobRequest, DownloadBlobResponse, GetBuildManifestRequest,
    GetBuildManifestResponse, SourceArchiveInfo, Timestamp,
};
use crate::repositories::{
    ApplicationBlobType, BlobRepository, ComponentRepository, SourceArchiveRepository,
};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

/// BuildService implementation
/// Provides build manifests and blob streaming downloads
#[derive(Clone)]
pub struct BuildServiceImpl {
    component_repo: Arc<ComponentRepository>,
    source_archive_repo: Arc<SourceArchiveRepository>,
    blob_repo: Arc<BlobRepository>,
    // TODO: Add RbacService in Phase 4 for permission checking
}

impl BuildServiceImpl {
    pub fn new(
        component_repo: Arc<ComponentRepository>,
        source_archive_repo: Arc<SourceArchiveRepository>,
        blob_repo: Arc<BlobRepository>,
    ) -> Self {
        Self {
            component_repo,
            source_archive_repo,
            blob_repo,
        }
    }

    /// Convert entity timestamp to proto timestamp
    fn to_proto_timestamp(dt: &sea_orm::prelude::DateTimeWithTimeZone) -> Option<Timestamp> {
        let ts = dt.timestamp();
        Some(Timestamp {
            seconds: ts,
            nanos: dt.timestamp_subsec_nanos() as i32,
        })
    }
}

#[tonic::async_trait]
impl BuildService for BuildServiceImpl {
    async fn get_build_manifest(
        &self,
        request: Request<GetBuildManifestRequest>,
    ) -> Result<Response<GetBuildManifestResponse>, Status> {
        let req = request.into_inner();

        let _actor = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // TODO: Phase 4 - Check actor has ComponentRead permission

        // 1. Get component info
        let component = self
            .component_repo
            .get_component(&component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get component");
                Status::internal(format!("Failed to get component: {}", e))
            })?
            .ok_or_else(|| Status::not_found(format!("Component not found: {}", component_id.id)))?;

        // 2. Get all source archives
        let archives = self
            .source_archive_repo
            .list_for_component(&component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list source archives");
                Status::internal(format!("Failed to list source archives: {}", e))
            })?;

        // 3. Get all component files organized by kind
        let files = self
            .component_repo
            .get_all_component_files(&component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get component files");
                Status::internal(format!("Failed to get component files: {}", e))
            })?;

        // 4. Build manifest
        let manifest = BuildManifest {
            component_id: Some(super::proto::ComponentId {
                id: component.id.clone(),
            }),
            component_name: component.name,
            recipe_kdl: component.recipe_kdl,
            source_archives: archives
                .into_iter()
                .map(|a| SourceArchiveInfo {
                    id: a.id,
                    component_id: a.component_id,
                    filename: a.filename,
                    url: a.url,
                    hash: Some(ContentHash { hex: a.blob_hash }),
                    size_bytes: a.size_bytes,
                    created_at: Self::to_proto_timestamp(&a.created_at),
                })
                .collect(),
            patches: files
                .patches
                .into_iter()
                .map(|f| ComponentFileInfo {
                    id: f.id,
                    component_id: f.component_id,
                    kind: f.kind,
                    name: f.name,
                    rel_path: f.rel_path,
                    hash: Some(ContentHash { hex: f.blob_hash }),
                    size_bytes: f.size_bytes,
                    created_at: Self::to_proto_timestamp(&f.created_at),
                })
                .collect(),
            licenses: files
                .licenses
                .into_iter()
                .map(|f| ComponentFileInfo {
                    id: f.id,
                    component_id: f.component_id,
                    kind: f.kind,
                    name: f.name,
                    rel_path: f.rel_path,
                    hash: Some(ContentHash { hex: f.blob_hash }),
                    size_bytes: f.size_bytes,
                    created_at: Self::to_proto_timestamp(&f.created_at),
                })
                .collect(),
            scripts: files
                .scripts
                .into_iter()
                .map(|f| ComponentFileInfo {
                    id: f.id,
                    component_id: f.component_id,
                    kind: f.kind,
                    name: f.name,
                    rel_path: f.rel_path,
                    hash: Some(ContentHash { hex: f.blob_hash }),
                    size_bytes: f.size_bytes,
                    created_at: Self::to_proto_timestamp(&f.created_at),
                })
                .collect(),
        };

        tracing::info!(
            component_id = %component_id.id,
            source_archives = manifest.source_archives.len(),
            patches = manifest.patches.len(),
            licenses = manifest.licenses.len(),
            scripts = manifest.scripts.len(),
            "Generated build manifest"
        );

        let response = GetBuildManifestResponse {
            manifest: Some(manifest),
        };

        Ok(Response::new(response))
    }

    type DownloadBlobStream =
        Pin<Box<dyn Stream<Item = Result<DownloadBlobResponse, Status>> + Send>>;

    async fn download_blob(
        &self,
        request: Request<DownloadBlobRequest>,
    ) -> Result<Response<Self::DownloadBlobStream>, Status> {
        let req = request.into_inner();

        let _actor = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let hash = req
            .hash
            .ok_or_else(|| Status::invalid_argument("hash is required"))?;

        // Parse blob type
        let blob_type: ApplicationBlobType =
            req.blob_type.parse().map_err(|e: String| {
                Status::invalid_argument(format!("invalid blob type: {}", e))
            })?;

        // TODO: Phase 4 - Check actor has ComponentRead permission for the blob

        // Get blob data
        let blob_repo = self.blob_repo.clone();
        let hash_hex = hash.hex.clone();

        let data = blob_repo
            .get_blob(&hash_hex, blob_type)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, hash = %hash_hex, "Failed to get blob");
                Status::not_found(format!("Blob not found: {}", hash_hex))
            })?;

        let total_size = data.len() as i64;

        tracing::info!(
            hash = %hash_hex,
            blob_type = %blob_type,
            size_bytes = total_size,
            "Starting blob download"
        );

        // Stream blob in 64KB chunks
        const CHUNK_SIZE: usize = 64 * 1024;

        let (tx, rx) = tokio::sync::mpsc::channel(4);

        // Spawn task to stream chunks
        tokio::spawn(async move {
            let mut offset = 0i64;
            let mut first = true;

            for chunk in data.chunks(CHUNK_SIZE) {
                let response = DownloadBlobResponse {
                    chunk: chunk.to_vec(),
                    total_size: if first { total_size } else { 0 },
                    offset,
                };

                if tx.send(Ok(response)).await.is_err() {
                    // Client disconnected
                    tracing::warn!(hash = %hash_hex, "Client disconnected during download");
                    break;
                }

                offset += chunk.len() as i64;
                first = false;
            }

            tracing::debug!(
                hash = %hash_hex,
                bytes_sent = offset,
                "Blob download completed"
            );
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(stream) as Self::DownloadBlobStream
        ))
    }
}
