use super::middleware::extract_actor;
use super::proto::{
    gate_service_server::GateService, AddMemberRequest, AddMemberResponse, ComponentInfo,
    CreateGateRequest, CreateGateResponse, GateInfo, GateMemberInfo, GetGateRequest,
    GetGateResponse, ListComponentsRequest, ListComponentsResponse, ListGatesRequest,
    ListGatesResponse, ListMembersRequest, ListMembersResponse, RemoveMemberRequest,
    RemoveMemberResponse, Timestamp, UpdateGateRequest, UpdateGateResponse,
};
use crate::pagination::{decode_cursor, encode_cursor, resolve_page_size};
use crate::repositories::{ComponentRepository, GateRepository};
use crate::services::RbacService;
use std::sync::Arc;
use tonic::{Request, Response, Status};

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

/// GateService implementation
/// Handles gate operations and member management
#[derive(Clone)]
pub struct GateServiceImpl {
    gate_repo: Arc<GateRepository>,
    component_repo: Arc<ComponentRepository>,
    rbac: Arc<RbacService>,
}

impl GateServiceImpl {
    pub fn new(
        gate_repo: Arc<GateRepository>,
        component_repo: Arc<ComponentRepository>,
        rbac: Arc<RbacService>,
    ) -> Self {
        Self {
            gate_repo,
            component_repo,
            rbac,
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
impl GateService for GateServiceImpl {
    async fn create_gate(
        &self,
        request: Request<CreateGateRequest>,
    ) -> Result<Response<CreateGateResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        // Validate inputs
        validate_name(&req.name)?;
        validate_kdl(&req.gate_kdl)?;

        // Any authenticated user can create a gate (they become the owner)
        let gate = self
            .gate_repo
            .create_gate(req.name, req.gate_kdl, actor.actor_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to create gate");
                Status::internal(format!("Failed to create gate: {}", e))
            })?;

        let response = CreateGateResponse {
            gate: Some(GateInfo {
                id: gate.id,
                name: gate.name,
                gate_kdl: gate.gate_kdl,
                owner_id: gate.owner_id,
                created_at: Self::to_proto_timestamp(&gate.created_at),
                updated_at: Self::to_proto_timestamp(&gate.updated_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn get_gate(
        &self,
        request: Request<GetGateRequest>,
    ) -> Result<Response<GetGateResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        // Check actor has GateRead permission
        let has_perm = self
            .rbac
            .check_gate_read(&actor.actor_id, &gate_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;

        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have read permission for this gate.",
            ));
        }

        let gate = self
            .gate_repo
            .get_gate(&gate_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get gate");
                Status::internal(format!("Failed to get gate: {}", e))
            })?
            .ok_or_else(|| Status::not_found(format!("Gate not found: {}", gate_id.id)))?;

        let response = GetGateResponse {
            gate: Some(GateInfo {
                id: gate.id,
                name: gate.name,
                gate_kdl: gate.gate_kdl,
                owner_id: gate.owner_id,
                created_at: Self::to_proto_timestamp(&gate.created_at),
                updated_at: Self::to_proto_timestamp(&gate.updated_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn update_gate(
        &self,
        request: Request<UpdateGateRequest>,
    ) -> Result<Response<UpdateGateResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        // Check actor has GateWrite permission
        let has_perm = self
            .rbac
            .check_gate_write(&actor.actor_id, &gate_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;

        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have write permission for this gate.",
            ));
        }

        // Validate KDL content
        validate_kdl(&req.gate_kdl)?;

        let gate = self
            .gate_repo
            .update_gate(&gate_id.id, req.gate_kdl)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to update gate");
                Status::internal(format!("Failed to update gate: {}", e))
            })?;

        let response = UpdateGateResponse {
            gate: Some(GateInfo {
                id: gate.id,
                name: gate.name,
                gate_kdl: gate.gate_kdl,
                owner_id: gate.owner_id,
                created_at: Self::to_proto_timestamp(&gate.created_at),
                updated_at: Self::to_proto_timestamp(&gate.updated_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn add_member(
        &self,
        request: Request<AddMemberRequest>,
    ) -> Result<Response<AddMemberResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        // Check actor has GateAdmin permission
        let has_perm = self
            .rbac
            .check_gate_admin(&actor.actor_id, &gate_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;

        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have admin permission for this gate.\n\
                 Only gate owners and admins can manage members.",
            ));
        }

        let member = self
            .gate_repo
            .add_member(
                &gate_id.id,
                &req.member_actor_id,
                req.roles,
                req.permissions,
            )
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to add member");
                Status::internal(format!("Failed to add member: {}", e))
            })?;

        let response = AddMemberResponse {
            member: Some(GateMemberInfo {
                gate_id: member.gate_id,
                actor_id: member.actor_id,
                roles: member
                    .roles
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                permissions: member
                    .permissions
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                created_at: Self::to_proto_timestamp(&member.created_at),
            }),
        };

        Ok(Response::new(response))
    }

    async fn remove_member(
        &self,
        request: Request<RemoveMemberRequest>,
    ) -> Result<Response<RemoveMemberResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        // Check actor has GateAdmin permission
        let has_perm = self
            .rbac
            .check_gate_admin(&actor.actor_id, &gate_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;

        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have admin permission for this gate.\n\
                 Only gate owners and admins can manage members.",
            ));
        }

        self.gate_repo
            .remove_member(&gate_id.id, &req.member_actor_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to remove member");
                Status::internal(format!("Failed to remove member: {}", e))
            })?;

        let response = RemoveMemberResponse { success: true };

        Ok(Response::new(response))
    }

    async fn list_members(
        &self,
        request: Request<ListMembersRequest>,
    ) -> Result<Response<ListMembersResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        let page_size = resolve_page_size(req.page_size, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        let cursor = decode_cursor(&req.page_token);

        // Check actor has GateRead permission
        let has_perm = self
            .rbac
            .check_gate_read(&actor.actor_id, &gate_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;

        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have read permission for this gate.",
            ));
        }

        let all_members = self
            .gate_repo
            .list_members(&gate_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list members");
                Status::internal(format!("Failed to list members: {}", e))
            })?;

        // Apply cursor-based pagination
        let filtered: Vec<_> = if let Some(after) = cursor {
            all_members
                .into_iter()
                .filter(|m| m.created_at.with_timezone(&chrono::Utc) > after)
                .collect()
        } else {
            all_members
        };

        let has_more = filtered.len() > page_size as usize;
        let members: Vec<_> = filtered
            .into_iter()
            .take(page_size as usize)
            .collect();

        let next_page_token = if has_more {
            members
                .last()
                .map(|m| encode_cursor(&m.created_at.with_timezone(&chrono::Utc)))
                .unwrap_or_default()
        } else {
            String::new()
        };

        let response = ListMembersResponse {
            members: members
                .into_iter()
                .map(|m| GateMemberInfo {
                    gate_id: m.gate_id,
                    actor_id: m.actor_id,
                    roles: m
                        .roles
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    permissions: m
                        .permissions
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    created_at: Self::to_proto_timestamp(&m.created_at),
                })
                .collect(),
            next_page_token,
        };

        Ok(Response::new(response))
    }

    async fn list_components(
        &self,
        request: Request<ListComponentsRequest>,
    ) -> Result<Response<ListComponentsResponse>, Status> {
        let actor = extract_actor(&request)?;
        let req = request.into_inner();

        let gate_id = req
            .gate_id
            .ok_or_else(|| Status::invalid_argument("gate_id is required"))?;

        let page_size = resolve_page_size(req.page_size, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        let cursor = decode_cursor(&req.page_token);

        // Check actor has GateRead permission
        let has_perm = self
            .rbac
            .check_gate_read(&actor.actor_id, &gate_id.id)
            .await
            .map_err(|e| Status::internal(format!("Permission check failed: {}", e)))?;

        if !has_perm {
            return Err(Status::permission_denied(
                "You do not have read permission for this gate.",
            ));
        }

        let all_components = self
            .component_repo
            .list_by_gate(&gate_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list components");
                Status::internal(format!("Failed to list components: {}", e))
            })?;

        // Apply cursor-based pagination
        let filtered: Vec<_> = if let Some(after) = cursor {
            all_components
                .into_iter()
                .filter(|c| c.created_at.with_timezone(&chrono::Utc) > after)
                .collect()
        } else {
            all_components
        };

        let has_more = filtered.len() > page_size as usize;
        let components: Vec<_> = filtered
            .into_iter()
            .take(page_size as usize)
            .collect();

        let next_page_token = if has_more {
            components
                .last()
                .map(|c| encode_cursor(&c.created_at.with_timezone(&chrono::Utc)))
                .unwrap_or_default()
        } else {
            String::new()
        };

        let response = ListComponentsResponse {
            components: components
                .into_iter()
                .map(|c| ComponentInfo {
                    id: c.id,
                    gate_id: c.gate_id,
                    name: c.name,
                    recipe_kdl: c.recipe_kdl,
                    created_at: Self::to_proto_timestamp(&c.created_at),
                    updated_at: Self::to_proto_timestamp(&c.updated_at),
                })
                .collect(),
            next_page_token,
        };

        Ok(Response::new(response))
    }

    async fn list_gates(
        &self,
        request: Request<ListGatesRequest>,
    ) -> Result<Response<ListGatesResponse>, Status> {
        let _actor = extract_actor(&request)?;
        let req = request.into_inner();

        let page_size = resolve_page_size(req.page_size, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        let cursor = decode_cursor(&req.page_token);

        // List gates: if owner_id specified, filter by owner.
        // Otherwise list all gates the actor has access to.
        let all_gates = if let Some(owner_id) = req.owner_id {
            self.gate_repo.list_by_owner(&owner_id).await
        } else {
            self.gate_repo.list_all().await
        }
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list gates");
            Status::internal(format!("Failed to list gates: {}", e))
        })?;

        // Apply cursor-based pagination
        let filtered: Vec<_> = if let Some(after) = cursor {
            all_gates
                .into_iter()
                .filter(|g| g.created_at.with_timezone(&chrono::Utc) > after)
                .collect()
        } else {
            all_gates
        };

        let has_more = filtered.len() > page_size as usize;
        let gates: Vec<_> = filtered
            .into_iter()
            .take(page_size as usize)
            .collect();

        let next_page_token = if has_more {
            gates
                .last()
                .map(|g| encode_cursor(&g.created_at.with_timezone(&chrono::Utc)))
                .unwrap_or_default()
        } else {
            String::new()
        };

        let response = ListGatesResponse {
            gates: gates
                .into_iter()
                .map(|g| GateInfo {
                    id: g.id,
                    name: g.name,
                    gate_kdl: g.gate_kdl,
                    owner_id: g.owner_id,
                    created_at: Self::to_proto_timestamp(&g.created_at),
                    updated_at: Self::to_proto_timestamp(&g.updated_at),
                })
                .collect(),
            next_page_token,
        };

        Ok(Response::new(response))
    }
}
