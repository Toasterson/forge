pub mod grpc;

pub use grpc::{
    extract_actor, start_server as start_grpc_server, AuthServiceImpl, AuthenticatedActor,
    BuildServiceImpl, ComponentServiceImpl, GateServiceImpl,
};

#[cfg(feature = "quic")]
pub mod quic {
    use miette::Result;
    use std::net::SocketAddr;
    use tracing::info;

    /// Placeholder for QUIC endpoint - deferred to post-MVP
    pub async fn start_quic_endpoint(addr: SocketAddr) -> Result<()> {
        info!(%addr, "QUIC endpoint not yet implemented - deferred to post-MVP");
        Err(miette::miette!(
            "QUIC endpoint not yet implemented - deferred to post-MVP"
        ))
    }
}
