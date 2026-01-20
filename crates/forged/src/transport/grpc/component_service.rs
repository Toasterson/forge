use super::proto::{
    component_service_server::ComponentService, upload_component_file_request, upload_source_archive_request, ComponentFileInfo, ComponentInfo, ContentHash,
    CreateComponentRequest, CreateComponentResponse, GetComponentRequest, GetComponentResponse,
    ListComponentFilesRequest, ListComponentFilesResponse, ListSourceArchivesRequest,
    ListSourceArchivesResponse, SourceArchiveInfo, Timestamp, UpdateComponentRequest,
    UpdateComponentResponse, UploadComponentFileRequest, UploadComponentFileResponse,
    UploadSourceArchiveRequest, UploadSourceArchiveResponse,
};
use crate::repositories::{ApplicationBlobType, ComponentRepository, SourceArchiveRepository};
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};

/// ComponentService implementation
/// Handles component CRUD and file uploads
#[derive(Clone)]
pub struct ComponentServiceImpl {
    component_repo: Arc<ComponentRepository>,
    source_archive_repo: Arc<SourceArchiveRepository>,
    // TODO: Add RbacService in Phase 4 for permission checking
}

impl ComponentServiceImpl {
    pub fn new(
        component_repo: Arc<ComponentRepository>,
        source_archive_repo: Arc<SourceArchiveRepository>,
    ) -> Self {
        Self {
            component_repo,
            source_archive_repo,
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
impl ComponentService for ComponentServiceImpl {
    async fn create_component(
        &self,
        request: Request<CreateComponentRequest>,
    ) -> Result<Response<CreateComponentResponse>, Status> {
        let req = request.into_inner();

        let _actor = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        // TODO: Phase 4 - Check actor has ComponentWrite permission for this gate

        let component = self
            .component_repo
            .create_component(&gate_id.id, req.name, req.recipe_kdl)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to create component");
                Status::internal(format!("Failed to create component: {}", e))
            })?;

        let response = CreateComponentResponse {
            component: Some(ComponentInfo {
                id: component.id,
                gate_id: component.gate_id,
                name: component.name,
                recipe_kdl: component.recipe_kdl,
                created_at: Self::to_proto_timestamp(&component.created_at),
                updated_at: Self::to_proto_timestamp(&component.updated_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn get_component(
        &self,
        request: Request<GetComponentRequest>,
    ) -> Result<Response<GetComponentResponse>, Status> {
        let req = request.into_inner();

        let _actor = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // TODO: Phase 4 - Check actor has ComponentRead permission

        let component = self
            .component_repo
            .get_component(&component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get component");
                Status::internal(format!("Failed to get component: {}", e))
            })?
            .ok_or_else(|| Status::not_found(format!("Component not found: {}", component_id.id)))?;

        let response = GetComponentResponse {
            component: Some(ComponentInfo {
                id: component.id,
                gate_id: component.gate_id,
                name: component.name,
                recipe_kdl: component.recipe_kdl,
                created_at: Self::to_proto_timestamp(&component.created_at),
                updated_at: Self::to_proto_timestamp(&component.updated_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn update_component(
        &self,
        request: Request<UpdateComponentRequest>,
    ) -> Result<Response<UpdateComponentResponse>, Status> {
        let req = request.into_inner();

        let _actor = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // TODO: Phase 4 - Check actor has ComponentWrite permission

        let component = self
            .component_repo
            .update_component(&component_id.id, req.recipe_kdl)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to update component");
                Status::internal(format!("Failed to update component: {}", e))
            })?;

        let response = UpdateComponentResponse {
            component: Some(ComponentInfo {
                id: component.id,
                gate_id: component.gate_id,
                name: component.name,
                recipe_kdl: component.recipe_kdl,
                created_at: Self::to_proto_timestamp(&component.created_at),
                updated_at: Self::to_proto_timestamp(&component.updated_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn upload_source_archive(
        &self,
        request: Request<Streaming<UploadSourceArchiveRequest>>,
    ) -> Result<Response<UploadSourceArchiveResponse>, Status> {
        let mut stream = request.into_inner();

        // First message contains metadata
        let first_msg = stream
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("empty stream"))?;

        let metadata = match first_msg.data {
            Some(upload_source_archive_request::Data::Metadata(m)) => m,
            _ => return Err(Status::invalid_argument("first message must contain metadata")),
        };

        let _actor = metadata
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = metadata
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // TODO: Phase 4 - Check actor has ComponentWrite permission

        // Collect all chunks
        let mut data = Vec::with_capacity(metadata.total_size as usize);

        while let Some(msg) = stream.message().await? {
            match msg.data {
                Some(upload_source_archive_request::Data::Chunk(chunk)) => {
                    data.extend_from_slice(&chunk);
                }
                _ => {
                    return Err(Status::invalid_argument(
                        "unexpected message type after metadata",
                    ))
                }
            }
        }

        // Verify size
        if data.len() != metadata.total_size as usize {
            return Err(Status::invalid_argument(format!(
                "size mismatch: expected {}, got {}",
                metadata.total_size,
                data.len()
            )));
        }

        // Store archive
        let archive = self
            .source_archive_repo
            .add_source_archive(&component_id.id, metadata.filename, metadata.url, &data)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to upload source archive");
                Status::internal(format!("Failed to upload source archive: {}", e))
            })?;

        let response = UploadSourceArchiveResponse {
            archive: Some(SourceArchiveInfo {
                id: archive.id,
                component_id: archive.component_id,
                filename: archive.filename,
                url: archive.url,
                hash: Some(ContentHash {
                    hex: archive.blob_hash,
                }),
                size_bytes: archive.size_bytes,
                created_at: Self::to_proto_timestamp(&archive.created_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn upload_component_file(
        &self,
        request: Request<Streaming<UploadComponentFileRequest>>,
    ) -> Result<Response<UploadComponentFileResponse>, Status> {
        let mut stream = request.into_inner();

        // First message contains metadata
        let first_msg = stream
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("empty stream"))?;

        let metadata = match first_msg.data {
            Some(upload_component_file_request::Data::Metadata(m)) => m,
            _ => return Err(Status::invalid_argument("first message must contain metadata")),
        };

        let _actor = metadata
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = metadata
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // Parse kind
        let kind: ApplicationBlobType = metadata.kind.parse().map_err(|e: String| {
            Status::invalid_argument(format!("invalid file kind: {}", e))
        })?;

        // TODO: Phase 4 - Check actor has ComponentWrite permission

        // Collect all chunks
        let mut data = Vec::with_capacity(metadata.total_size as usize);

        while let Some(msg) = stream.message().await? {
            match msg.data {
                Some(upload_component_file_request::Data::Chunk(chunk)) => {
                    data.extend_from_slice(&chunk);
                }
                _ => {
                    return Err(Status::invalid_argument(
                        "unexpected message type after metadata",
                    ))
                }
            }
        }

        // Verify size
        if data.len() != metadata.total_size as usize {
            return Err(Status::invalid_argument(format!(
                "size mismatch: expected {}, got {}",
                metadata.total_size,
                data.len()
            )));
        }

        // Store file
        let file = self
            .component_repo
            .add_component_file(&component_id.id, kind, metadata.name, metadata.rel_path, &data)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to upload component file");
                Status::internal(format!("Failed to upload component file: {}", e))
            })?;

        let response = UploadComponentFileResponse {
            file: Some(ComponentFileInfo {
                id: file.id,
                component_id: file.component_id,
                kind: file.kind,
                name: file.name,
                rel_path: file.rel_path,
                hash: Some(ContentHash { hex: file.blob_hash }),
                size_bytes: file.size_bytes,
                created_at: Self::to_proto_timestamp(&file.created_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn list_source_archives(
        &self,
        request: Request<ListSourceArchivesRequest>,
    ) -> Result<Response<ListSourceArchivesResponse>, Status> {
        let req = request.into_inner();

        let _actor = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // TODO: Phase 4 - Check actor has ComponentRead permission

        let archives = self
            .source_archive_repo
            .list_for_component(&component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list source archives");
                Status::internal(format!("Failed to list source archives: {}", e))
            })?;

        let response = ListSourceArchivesResponse {
            archives: archives
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
        };

        Ok(Response::new(response))
    }

    async fn list_component_files(
        &self,
        request: Request<ListComponentFilesRequest>,
    ) -> Result<Response<ListComponentFilesResponse>, Status> {
        let req = request.into_inner();

        let _actor = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // Parse optional kind filter
        let kind = if let Some(k) = req.kind {
            Some(k.parse::<ApplicationBlobType>().map_err(|e: String| {
                Status::invalid_argument(format!("invalid file kind: {}", e))
            })?)
        } else {
            None
        };

        // TODO: Phase 4 - Check actor has ComponentRead permission

        let files = self
            .component_repo
            .list_component_files(&component_id.id, kind)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list component files");
                Status::internal(format!("Failed to list component files: {}", e))
            })?;

        let response = ListComponentFilesResponse {
            files: files
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

        Ok(Response::new(response))
    }
}
