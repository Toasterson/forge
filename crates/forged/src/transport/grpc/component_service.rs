use super::middleware::extract_actor;
use super::proto::{
    component_service_server::ComponentService, upload_component_file_request,
    upload_source_archive_request, ComponentFileInfo, ComponentInfo, ContentHash,
    CreateComponentRequest, CreateComponentResponse, GetComponentRequest, GetComponentResponse,
    ListComponentFilesRequest, ListComponentFilesResponse, ListSourceArchivesRequest,
    ListSourceArchivesResponse, SourceArchiveInfo, Timestamp, UpdateComponentRequest,
    UpdateComponentResponse, UploadComponentFileRequest, UploadComponentFileResponse,
    UploadSourceArchiveRequest, UploadSourceArchiveResponse,
};
use crate::pagination::{decode_cursor, encode_cursor, resolve_page_size};
use crate::repositories::{ApplicationBlobType, ComponentRepository, SourceArchiveRepository};
use crate::services::RbacService;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};

/// Default page size for paginated list requests.
const DEFAULT_PAGE_SIZE: u32 = 50;

/// Maximum allowed page size for paginated list requests.
const MAX_PAGE_SIZE: u32 = 1000;

/// Maximum allowed length for a name field (256 characters).
const MAX_NAME_LEN: usize = 256;

/// Maximum allowed length for a KDL field (1 MiB).
const MAX_KDL_LEN: usize = 1024 * 1024;

/// Validate that a name field is non-empty and within the maximum length.
fn validate_name(name: &str) -> Result<(), Status> {
    if name.is_empty() {
        return Err(Status::invalid_argument(
            "name must not be empty.\n\
             Provide a non-empty name for this resource.",
        ));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(Status::invalid_argument(format!(
            "name exceeds maximum length of {} characters (got {}).\n\
             Shorten the name to at most {} characters.",
            MAX_NAME_LEN,
            name.len(),
            MAX_NAME_LEN,
        )));
    }
    Ok(())
}

/// Validate that a KDL content field is within the maximum length (1 MiB).
fn validate_kdl(kdl: &str) -> Result<(), Status> {
    if kdl.len() > MAX_KDL_LEN {
        return Err(Status::invalid_argument(format!(
            "KDL content exceeds maximum size of 1 MiB (got {} bytes).\n\
             Reduce the KDL content size to at most {} bytes.",
            kdl.len(),
            MAX_KDL_LEN,
        )));
    }
    Ok(())
}

/// ComponentService implementation
/// Handles component CRUD and file uploads
#[derive(Clone)]
pub struct ComponentServiceImpl {
    component_repo: Arc<ComponentRepository>,
    source_archive_repo: Arc<SourceArchiveRepository>,
    rbac: Arc<RbacService>,
    max_upload_size: u64,
}

impl ComponentServiceImpl {
    pub fn new(
        component_repo: Arc<ComponentRepository>,
        source_archive_repo: Arc<SourceArchiveRepository>,
        rbac: Arc<RbacService>,
    ) -> Self {
        Self {
            component_repo,
            source_archive_repo,
            rbac,
            max_upload_size: 2 * 1024 * 1024 * 1024, // 2 GiB default
        }
    }

    pub fn with_max_upload_size(mut self, max_upload_size: u64) -> Self {
        self.max_upload_size = max_upload_size;
        self
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
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        // Validate inputs
        validate_name(&req.name)?;
        validate_kdl(&req.recipe_kdl)?;

        // Check actor has ComponentWrite permission for this gate
        let has_perm = self
            .rbac
            .check_gate_permission(
                &actor.actor_id,
                &gate_id.id,
                crate::services::GatePermission::ComponentWrite,
            )
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;
        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have component write permission for this gate.",
            ));
        }

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
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // Validate KDL content
        validate_kdl(&req.recipe_kdl)?;

        // Check actor has ComponentWrite permission
        let has_perm = self
            .rbac
            .check_component_write(&actor.actor_id, &component_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;
        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have write permission for this component.",
            ));
        }

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
            _ => {
                return Err(Status::invalid_argument(
                    "first message must contain metadata",
                ))
            }
        };

        let _actor = metadata
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = metadata
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // Reject uploads exceeding the configured maximum
        if metadata.total_size as u64 > self.max_upload_size {
            return Err(Status::resource_exhausted(format!(
                "Upload size {} bytes exceeds maximum of {} bytes",
                metadata.total_size, self.max_upload_size
            )));
        }

        // Check actor has ComponentWrite permission
        let has_perm = self
            .rbac
            .check_component_write(&_actor.id, &component_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;
        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have write permission for this component.",
            ));
        }

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
            _ => {
                return Err(Status::invalid_argument(
                    "first message must contain metadata",
                ))
            }
        };

        let _actor = metadata
            .actor
            .ok_or_else(|| Status::invalid_argument("actor is required"))?;

        let component_id = metadata
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        // Parse kind
        let kind: ApplicationBlobType = metadata
            .kind
            .parse()
            .map_err(|e: String| Status::invalid_argument(format!("invalid file kind: {}", e)))?;

        // Check actor has ComponentWrite permission
        let has_perm = self
            .rbac
            .check_component_write(&_actor.id, &component_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;
        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have write permission for this component.",
            ));
        }

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
            .add_component_file(
                &component_id.id,
                kind,
                metadata.name,
                metadata.rel_path,
                &data,
            )
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
                hash: Some(ContentHash {
                    hex: file.blob_hash,
                }),
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
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let component_id = req
            .component_id
            .ok_or_else(|| Status::invalid_argument("component_id is required"))?;

        let page_size = resolve_page_size(req.page_size, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        let cursor = decode_cursor(&req.page_token);

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

        let all_archives = self
            .source_archive_repo
            .list_for_component(&component_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list source archives");
                Status::internal(format!("Failed to list source archives: {}", e))
            })?;

        // Apply cursor-based pagination
        let filtered: Vec<_> = if let Some(after) = cursor {
            all_archives
                .into_iter()
                .filter(|a| a.created_at.with_timezone(&chrono::Utc) > after)
                .collect()
        } else {
            all_archives
        };

        let has_more = filtered.len() > page_size as usize;
        let archives: Vec<_> = filtered
            .into_iter()
            .take(page_size as usize)
            .collect();

        let next_page_token = if has_more {
            archives
                .last()
                .map(|a| encode_cursor(&a.created_at.with_timezone(&chrono::Utc)))
                .unwrap_or_default()
        } else {
            String::new()
        };

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
            next_page_token,
        };

        Ok(Response::new(response))
    }

    async fn list_component_files(
        &self,
        request: Request<ListComponentFilesRequest>,
    ) -> Result<Response<ListComponentFilesResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

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

        let page_size = resolve_page_size(req.page_size, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        let cursor = decode_cursor(&req.page_token);

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

        let all_files = self
            .component_repo
            .list_component_files(&component_id.id, kind)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list component files");
                Status::internal(format!("Failed to list component files: {}", e))
            })?;

        // Apply cursor-based pagination
        let filtered: Vec<_> = if let Some(after) = cursor {
            all_files
                .into_iter()
                .filter(|f| f.created_at.with_timezone(&chrono::Utc) > after)
                .collect()
        } else {
            all_files
        };

        let has_more = filtered.len() > page_size as usize;
        let files: Vec<_> = filtered
            .into_iter()
            .take(page_size as usize)
            .collect();

        let next_page_token = if has_more {
            files
                .last()
                .map(|f| encode_cursor(&f.created_at.with_timezone(&chrono::Utc)))
                .unwrap_or_default()
        } else {
            String::new()
        };

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
            next_page_token,
        };

        Ok(Response::new(response))
    }
}
