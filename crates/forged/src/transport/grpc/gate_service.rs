use super::middleware::extract_actor;
use super::proto::{
    gate_service_server::GateService, AddMemberRequest, AddMemberResponse, ComponentInfo,
    CreateGateRequest, CreateGateResponse, GateInfo, GateMemberInfo, GetGateRequest,
    GetGateResponse, ListComponentsRequest, ListComponentsResponse, ListGatesRequest,
    ListGatesResponse, ListMembersRequest, ListMembersResponse, RemoveMemberRequest,
    RemoveMemberResponse, Timestamp, UpdateGateRequest, UpdateGateResponse,
};
use crate::repositories::{ComponentRepository, GateRepository};
use crate::services::RbacService;
use std::sync::Arc;
use tonic::{Request, Response, Status};

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

        let members = self
            .gate_repo
            .list_members(&gate_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list members");
                Status::internal(format!("Failed to list members: {}", e))
            })?;

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

        let components = self
            .component_repo
            .list_by_gate(&gate_id.id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to list components");
                Status::internal(format!("Failed to list components: {}", e))
            })?;

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
        };

        Ok(Response::new(response))
    }

    async fn list_gates(
        &self,
        request: Request<ListGatesRequest>,
    ) -> Result<Response<ListGatesResponse>, Status> {
        let _actor = extract_actor(&request)?;
        let req = request.into_inner();

        // List gates: if owner_id specified, filter by owner.
        // Otherwise list all gates the actor has access to.
        let gates = if let Some(owner_id) = req.owner_id {
            self.gate_repo.list_by_owner(&owner_id).await
        } else {
            self.gate_repo.list_all().await
        }
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list gates");
            Status::internal(format!("Failed to list gates: {}", e))
        })?;

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
        };

        Ok(Response::new(response))
    }
}
