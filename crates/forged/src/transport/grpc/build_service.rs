use super::middleware::extract_actor;
use super::proto::{
    build_service_server::BuildService, BuildJobInfo, BuildManifest, CancelBuildRequest,
    CancelBuildResponse, ComponentFileInfo, ContentHash, DownloadBlobRequest, DownloadBlobResponse,
    GetBuildManifestRequest, GetBuildManifestResponse, GetBuildStatusRequest,
    GetBuildStatusResponse, ListBuildsRequest, ListBuildsResponse, SourceArchiveInfo,
    SubmitBuildRequest, SubmitBuildResponse, Timestamp,
};
use crate::pagination::{decode_cursor, encode_cursor, resolve_page_size};
use crate::repositories::{
    ApplicationBlobType, BlobRepository, ComponentRepository, SourceArchiveRepository,
};
use crate::services::{BuildDispatchService, RbacService};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

/// Default page size for paginated list requests.
const DEFAULT_PAGE_SIZE: u32 = 50;

/// Maximum allowed page size for paginated list requests.
const MAX_PAGE_SIZE: u32 = 1000;

/// BuildService implementation
/// Provides build manifests, blob streaming downloads, and build dispatch
#[derive(Clone)]
pub struct BuildServiceImpl {
    component_repo: Arc<ComponentRepository>,
    source_archive_repo: Arc<SourceArchiveRepository>,
    blob_repo: Arc<BlobRepository>,
    rbac: Arc<RbacService>,
    build_dispatch: Arc<BuildDispatchService>,
}

impl BuildServiceImpl {
    pub fn new(
        component_repo: Arc<ComponentRepository>,
        source_archive_repo: Arc<SourceArchiveRepository>,
        blob_repo: Arc<BlobRepository>,
        rbac: Arc<RbacService>,
        build_dispatch: Arc<BuildDispatchService>,
    ) -> Self {
        Self {
            component_repo,
            source_archive_repo,
            blob_repo,
            rbac,
            build_dispatch,
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
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // Check actor has ComponentRead permission
        let has_perm = self
            .rbac
            .check_component_read(&actor.actor_id, &component_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;
        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have read permission for this component.",
            ));
        }

        // 1. Get component info
        let component = self
            .component_repo
            .get_component(&component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get component");
                Status::internal(format!("Failed to get component: {}", e))
            })?
            .ok_or_else(|| {
                Status::not_found(format!("Component not found: {}", component_id.id))
            })?;

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
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let hash = req
            .hash
            .ok_or_else(|| Status::invalid_argument("hash is required"))?;

        // Parse blob type
        let blob_type: ApplicationBlobType = req
            .blob_type
            .parse()
            .map_err(|e: String| Status::invalid_argument(format!("invalid blob type: {}", e)))?;

        // Blob downloads require authentication (enforced by extract_actor above)
        // Fine-grained per-blob permission checking would require tracking blob ownership
        // which is done via component_file and source_archive tables.
        // For now, any authenticated user can download blobs they know the hash of.

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
        Ok(Response::new(Box::pin(stream) as Self::DownloadBlobStream))
    }

    async fn submit_build(
        &self,
        request: Request<SubmitBuildRequest>,
    ) -> Result<Response<SubmitBuildResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        let job = self
            .build_dispatch
            .submit_build(&actor.actor_id, &component_id.id, &gate_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to submit build");
                Status::internal(format!("Failed to submit build: {}", e))
            })?;

        Ok(Response::new(SubmitBuildResponse {
            job: Some(Self::to_build_job_info(&job)),
        }))
    }

    async fn get_build_status(
        &self,
        request: Request<GetBuildStatusRequest>,
    ) -> Result<Response<GetBuildStatusResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let job = self
            .build_dispatch
            .get_build_status(&actor.actor_id, &req.job_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get build status");
                Status::internal(format!("Failed to get build status: {}", e))
            })?;

        Ok(Response::new(GetBuildStatusResponse {
            job: Some(Self::to_build_job_info(&job)),
        }))
    }

    async fn list_builds(
        &self,
        request: Request<ListBuildsRequest>,
    ) -> Result<Response<ListBuildsResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        let page_size = resolve_page_size(req.page_size, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        let cursor = decode_cursor(&req.page_token);

        let all_jobs = self
            .build_dispatch
            .list_builds(&actor.actor_id, &component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list builds");
                Status::internal(format!("Failed to list builds: {}", e))
            })?;

        // Apply cursor-based pagination
        let filtered: Vec<_> = if let Some(after) = cursor {
            all_jobs
                .into_iter()
                .filter(|j| j.created_at.with_timezone(&chrono::Utc) > after)
                .collect()
        } else {
            all_jobs
        };

        let has_more = filtered.len() > page_size as usize;
        let jobs: Vec<_> = filtered.into_iter().take(page_size as usize).collect();

        let next_page_token = if has_more {
            jobs.last()
                .map(|j| encode_cursor(&j.created_at.with_timezone(&chrono::Utc)))
                .unwrap_or_default()
        } else {
            String::new()
        };

        Ok(Response::new(ListBuildsResponse {
            jobs: jobs.iter().map(Self::to_build_job_info).collect(),
            next_page_token,
        }))
    }

    async fn cancel_build(
        &self,
        request: Request<CancelBuildRequest>,
    ) -> Result<Response<CancelBuildResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let job = self
            .build_dispatch
            .cancel_build(&actor.actor_id, &req.job_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to cancel build");
                Status::internal(format!("Failed to cancel build: {}", e))
            })?;

        Ok(Response::new(CancelBuildResponse {
            job: Some(Self::to_build_job_info(&job)),
        }))
    }
}

impl BuildServiceImpl {
    fn to_build_job_info(job: &crate::entities::build_job::Model) -> BuildJobInfo {
        BuildJobInfo {
            id: job.id.clone(),
            component_id: job.component_id.clone(),
            gate_id: job.gate_id.clone(),
            actor_id: job.actor_id.clone(),
            request_id: job.request_id.clone(),
            status: job.status.clone(),
            exit_code: job.exit_code,
            summary: job.summary.clone(),
            build_log_url: job.build_log_url.clone(),
            created_at: Self::to_proto_timestamp(&job.created_at),
            updated_at: Self::to_proto_timestamp(&job.updated_at),
            completed_at: job.completed_at.as_ref().and_then(Self::to_proto_timestamp),
        }
    }
}
