use crate::repositories::ActorRepository;
use crate::services::OidcService;
use std::sync::Arc;
use tonic::{Request, Status};
use uuid::Uuid;

/// Reference to an authenticated actor, injected into request extensions by the auth middleware.
#[derive(Debug, Clone)]
pub struct AuthenticatedActor {
    pub actor_id: String,
    pub display_name: String,
}

/// gRPC paths that do not require authentication.
///
/// GetAuthConfig and IssueToken are public RPCs.
/// RegistrationConfirmation is kept public but returns UNIMPLEMENTED.
const UNAUTHENTICATED_METHODS: &[&str] = &[
    "/forged.api.v2.AuthService/RegistrationConfirmation",
    "/forged.api.v2.AuthService/IssueToken",
    "/forged.api.v2.AuthService/GetAuthConfig",
];

/// Extract the authenticated actor from request extensions.
///
/// Call this in each handler that requires authentication.
/// Returns `Status::unauthenticated` if no actor was injected by the middleware.
#[allow(clippy::result_large_err)]
pub fn extract_actor<T>(request: &Request<T>) -> Result<AuthenticatedActor, Status> {
    request
        .extensions()
        .get::<AuthenticatedActor>()
        .cloned()
        .ok_or_else(|| {
            Status::unauthenticated(
                "Authentication required.\n\
                 Include an 'authorization: Bearer <oidc_token>' metadata header \
                 in your gRPC request.\n\
                 Obtain a token from your OIDC provider.",
            )
        })
}

/// Check if a gRPC method path should skip authentication.
fn is_unauthenticated_method(path: &str) -> bool {
    UNAUTHENTICATED_METHODS.contains(&path)
}

/// Tower layer for async OIDC auth validation on gRPC requests.
///
/// Strategy: always forward the request. If a valid Bearer token is present,
/// inject `AuthenticatedActor` into extensions. Handlers use `extract_actor()`
/// to require authentication. This avoids constructing error responses in the
/// middleware layer.
pub mod tower_auth {
    use super::*;
    use futures::future::BoxFuture;
    use std::task::{Context, Poll};
    use tower::{Layer, Service};

    #[derive(Clone)]
    pub struct AuthLayer {
        oidc: Arc<OidcService>,
        actor_repo: Arc<ActorRepository>,
    }

    impl AuthLayer {
        pub fn new(oidc: Arc<OidcService>, actor_repo: Arc<ActorRepository>) -> Self {
            Self { oidc, actor_repo }
        }
    }

    impl<S> Layer<S> for AuthLayer {
        type Service = AuthMiddleware<S>;

        fn layer(&self, inner: S) -> Self::Service {
            AuthMiddleware {
                inner,
                oidc: self.oidc.clone(),
                actor_repo: self.actor_repo.clone(),
            }
        }
    }

    #[derive(Clone)]
    pub struct AuthMiddleware<S> {
        inner: S,
        oidc: Arc<OidcService>,
        actor_repo: Arc<ActorRepository>,
    }

    impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for AuthMiddleware<S>
    where
        S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
        S::Error: Send + 'static,
        ReqBody: Send + 'static,
    {
        type Response = S::Response;
        type Error = S::Error;
        type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, mut req: http::Request<ReqBody>) -> Self::Future {
            // Clone the inner service (required for tower correctness)
            let mut inner = self.inner.clone();
            std::mem::swap(&mut self.inner, &mut inner);

            let oidc = self.oidc.clone();
            let actor_repo = self.actor_repo.clone();

            Box::pin(async move {
                let path = req.uri().path().to_string();

                // Generate a unique request ID for tracing and response correlation
                let request_id = Uuid::new_v4().to_string();

                // Skip auth for public RPCs
                if !is_unauthenticated_method(&path) {
                    // Try to extract and validate bearer token
                    if let Some(token) = extract_bearer_token(&req) {
                        match oidc.validate_token(&token).await {
                            Ok(claims) => {
                                // Create or update actor, inject into extensions
                                match actor_repo
                                    .create_or_update_from_oidc(
                                        claims.subject,
                                        claims.display_name.clone(),
                                    )
                                    .await
                                {
                                    Ok(actor) => {
                                        req.extensions_mut().insert(AuthenticatedActor {
                                            actor_id: actor.id,
                                            display_name: actor.display_name,
                                        });
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            error = %e,
                                            "Failed to create/update actor from OIDC claims"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::debug!(
                                    error = %e,
                                    path = %path,
                                    "Bearer token validation failed"
                                );
                            }
                        }
                    }
                }

                let span = tracing::info_span!(
                    "grpc_request",
                    request_id = %request_id,
                    path = %path,
                );
                let _enter = span.enter();

                let mut response = inner.call(req).await?;

                // Propagate request ID in response headers
                response.headers_mut().insert(
                    "x-request-id",
                    http::HeaderValue::from_str(&request_id)
                        .unwrap_or_else(|_| http::HeaderValue::from_static("unknown")),
                );

                Ok(response)
            })
        }
    }

    /// Extract bearer token from the Authorization header.
    fn extract_bearer_token<B>(req: &http::Request<B>) -> Option<String> {
        let value = req.headers().get("authorization")?;
        let value_str = value.to_str().ok()?;
        let token = value_str.strip_prefix("Bearer ")?;
        Some(token.to_string())
    }
}
