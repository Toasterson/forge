// Generated protobuf code
pub mod proto {
    tonic::include_proto!("forged.api.v2");
}

pub mod auth_service;
pub mod build_service;
pub mod component_service;
pub mod gate_service;

pub use auth_service::AuthServiceImpl;
pub use build_service::BuildServiceImpl;
pub use component_service::ComponentServiceImpl;
pub use gate_service::GateServiceImpl;

use miette::{IntoDiagnostic, Result};
use std::net::SocketAddr;
use tonic::transport::Server;
use tracing::info;

/// Start the gRPC server with all services
pub async fn start_server(
    addr: SocketAddr,
    auth_service: AuthServiceImpl,
    gate_service: GateServiceImpl,
    component_service: ComponentServiceImpl,
    build_service: BuildServiceImpl,
) -> Result<()> {
    info!(%addr, "Starting gRPC server");

    Server::builder()
        .add_service(proto::auth_service_server::AuthServiceServer::new(
            auth_service,
        ))
        .add_service(proto::gate_service_server::GateServiceServer::new(
            gate_service,
        ))
        .add_service(proto::component_service_server::ComponentServiceServer::new(
            component_service,
        ))
        .add_service(proto::build_service_server::BuildServiceServer::new(
            build_service,
        ))
        .serve(addr)
        .await
        .into_diagnostic()?;

    Ok(())
}
