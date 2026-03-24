use super::middleware::extract_actor;
use super::proto::{
    auth_service_server::AuthService, ActorRef, AddActorKeyRequest, AddActorKeyResponse,
    AuthenticateRequest, AuthenticateResponse, GetAuthConfigRequest, GetAuthConfigResponse,
    IssueTokenRequest, IssueTokenResponse, RegisterActorRequest, RegisterActorResponse,
    RegistrationConfirmationRequest, RegistrationConfirmationResponse,
};
use crate::services::AuthService as AuthServiceLogic;
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// AuthService gRPC implementation
///
/// Delegates to `services::AuthService` for business logic.
/// OIDC token validation and SSH key registration/confirmation
/// are handled at the service layer.
#[derive(Clone)]
pub struct AuthServiceImpl {
    auth: Arc<AuthServiceLogic>,
}

impl AuthServiceImpl {
    pub fn new(auth: Arc<AuthServiceLogic>) -> Self {
        Self { auth }
    }
}

#[tonic::async_trait]
impl AuthService for AuthServiceImpl {
    async fn authenticate(
        &self,
        request: Request<AuthenticateRequest>,
    ) -> Result<Response<AuthenticateResponse>, Status> {
        let req = request.into_inner();

        let actor = self
            .auth
            .authenticate_oidc(&req.oidc_token)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "OIDC authentication failed");
                Status::unauthenticated(format!("{}", e))
            })?;

        tracing::info!(
            actor_id = %actor.id,
            display_name = %actor.display_name,
            "Actor authenticated via OIDC"
        );

        Ok(Response::new(AuthenticateResponse {
            actor: Some(ActorRef {
                id: actor.id,
                kind: actor.kind,
            }),
            display_name: actor.display_name,
        }))
    }

    async fn register_actor(
        &self,
        request: Request<RegisterActorRequest>,
    ) -> Result<Response<RegisterActorResponse>, Status> {
        // Requires OIDC authentication — the middleware injects the authenticated actor
        let authenticated = extract_actor(&request)?;
        let req = request.into_inner();

        if req.public_key.is_empty() {
            return Err(Status::invalid_argument(
                "public_key is required.\n\
                 Provide an SSH public key in OpenSSH format (e.g. 'ssh-ed25519 AAAA...').",
            ));
        }

        let key_id = if req.key_id.is_empty() {
            "default".to_string()
        } else {
            req.key_id
        };

        self.auth
            .add_ssh_key(&authenticated.actor_id, &key_id, &req.public_key)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, actor_id = %authenticated.actor_id, "SSH key registration failed");
                Status::internal(format!("{}", e))
            })?;

        tracing::info!(
            actor_id = %authenticated.actor_id,
            key_id = %key_id,
            "SSH key registered for OIDC-authenticated actor"
        );

        Ok(Response::new(RegisterActorResponse {
            actor: Some(ActorRef {
                id: authenticated.actor_id,
                kind: "user".to_string(),
            }),
            confirmation_envelope: String::new(),
        }))
    }

    async fn registration_confirmation(
        &self,
        _request: Request<RegistrationConfirmationRequest>,
    ) -> Result<Response<RegistrationConfirmationResponse>, Status> {
        Err(Status::unimplemented(
            "Registration confirmation is no longer required.\n\
             SSH keys are now added to OIDC-authenticated accounts directly via RegisterActor.",
        ))
    }

    async fn add_actor_key(
        &self,
        request: Request<AddActorKeyRequest>,
    ) -> Result<Response<AddActorKeyResponse>, Status> {
        let req = request.into_inner();

        let actor_ref = req
            .actor
            .ok_or_else(|| Status::invalid_argument("actor reference is required"))?;

        if req.public_key.is_empty() || req.proof_signature.is_empty() {
            return Err(Status::invalid_argument(
                "public_key and proof_signature are required to add a new key.",
            ));
        }

        let key_id = if req.key_id.is_empty() {
            "default".to_string()
        } else {
            req.key_id
        };

        let proof_key_id = if req.proof_key_id.is_empty() {
            "default"
        } else {
            &req.proof_key_id
        };

        use base64::Engine as _;
        let proof_bytes = base64::engine::general_purpose::STANDARD
            .decode(&req.proof_signature)
            .map_err(|e| {
                Status::invalid_argument(format!("proof_signature must be base64-encoded: {}", e))
            })?;

        self.auth
            .add_actor_key(
                &actor_ref.id,
                &req.public_key,
                key_id.clone(),
                &proof_bytes,
                proof_key_id,
            )
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "Add actor key failed");
                Status::permission_denied(format!("{}", e))
            })?;

        Ok(Response::new(AddActorKeyResponse {
            success: true,
            key_id,
        }))
    }

    async fn issue_token(
        &self,
        _request: Request<IssueTokenRequest>,
    ) -> Result<Response<IssueTokenResponse>, Status> {
        Err(Status::unimplemented(
            "Token issuance is handled by the OIDC provider, not by Forge.\n\
             Configure your OIDC provider and obtain a token from it.\n\
             See: https://forge.example.com/docs/auth for setup instructions.",
        ))
    }

    async fn get_auth_config(
        &self,
        _request: Request<GetAuthConfigRequest>,
    ) -> Result<Response<GetAuthConfigResponse>, Status> {
        Ok(Response::new(GetAuthConfigResponse {
            issuer_url: self.auth.oidc_issuer_url().to_string(),
            client_id: self.auth.oidc_client_id().to_string(),
        }))
    }
}
