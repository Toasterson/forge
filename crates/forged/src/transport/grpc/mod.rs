// Generated protobuf code
pub mod proto {
    tonic::include_proto!("forged.api.v2");
}

pub mod auth_service;
pub mod build_service;
pub mod component_service;
pub mod gate_service;
pub mod middleware;

pub use auth_service::AuthServiceImpl;
pub use build_service::BuildServiceImpl;
pub use component_service::ComponentServiceImpl;
pub use gate_service::GateServiceImpl;
pub use middleware::{extract_actor, AuthenticatedActor};

use crate::repositories::ActorRepository;
use crate::services::OidcService;
use miette::{IntoDiagnostic, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::info;

/// Start the gRPC server with all services and auth middleware
pub async fn start_server(
    addr: SocketAddr,
    auth_service: AuthServiceImpl,
    gate_service: GateServiceImpl,
    component_service: ComponentServiceImpl,
    build_service: BuildServiceImpl,
    oidc: Arc<OidcService>,
    actor_repo: Arc<ActorRepository>,
) -> Result<()> {
    info!(%addr, "Starting gRPC server");

    let auth_layer = middleware::tower_auth::AuthLayer::new(oidc, actor_repo);

    Server::builder()
        .layer(auth_layer)
        .add_service(proto::auth_service_server::AuthServiceServer::new(
            auth_service,
        ))
        .add_service(proto::gate_service_server::GateServiceServer::new(
            gate_service,
        ))
        .add_service(
            proto::component_service_server::ComponentServiceServer::new(component_service),
        )
        .add_service(proto::build_service_server::BuildServiceServer::new(
            build_service,
        ))
        .serve(addr)
        .await
        .into_diagnostic()?;

    Ok(())
}
