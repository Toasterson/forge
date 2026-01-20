use super::proto::{
    auth_service_server::AuthService, ActorRef, AuthenticateRequest, AuthenticateResponse,
};
use crate::repositories::ActorRepository;
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// AuthService implementation
/// Validates OIDC tokens and creates/updates actors
#[derive(Clone)]
pub struct AuthServiceImpl {
    actor_repo: Arc<ActorRepository>,
    // TODO: Add OidcService in Phase 4
}

impl AuthServiceImpl {
    pub fn new(actor_repo: Arc<ActorRepository>) -> Self {
        Self { actor_repo }
    }

    /// Validate OIDC token and extract claims
    /// TODO: Replace with actual OIDC validation in Phase 4
    async fn validate_token(&self, token: &str) -> Result<(String, String), Status> {
        // For now, this is a stub that will be replaced with real OIDC validation
        // Expected format for testing: "oidc_sub:display_name"

        if token.is_empty() {
            return Err(Status::unauthenticated("OIDC token is required"));
        }

        // Stub implementation - parse test token
        if let Some((sub, name)) = token.split_once(':') {
            Ok((sub.to_string(), name.to_string()))
        } else {
            // For simple tokens, use the token as both sub and display name
            Ok((token.to_string(), token.to_string()))
        }
    }
}

#[tonic::async_trait]
impl AuthService for AuthServiceImpl {
    async fn authenticate(
        &self,
        request: Request<AuthenticateRequest>,
    ) -> Result<Response<AuthenticateResponse>, Status> {
        let req = request.into_inner();

        // 1. Validate OIDC token
        let (oidc_sub, display_name) = self
            .validate_token(&req.oidc_token)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "Failed to validate OIDC token");
                e
            })?;

        // 2. Create or update actor
        let actor = self
            .actor_repo
            .create_or_update_from_oidc(oidc_sub, display_name.clone())
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to create/update actor");
                Status::internal(format!("Failed to create/update actor: {}", e))
            })?;

        tracing::info!(
            actor_id = %actor.id,
            display_name = %display_name,
            "Actor authenticated"
        );

        // 3. Return actor reference
        let response = AuthenticateResponse {
            actor: Some(ActorRef {
                id: actor.id,
                kind: actor.kind,
            }),
            display_name: actor.display_name,
        };

        Ok(Response::new(response))
    }
}
