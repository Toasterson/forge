// TODO: This entire module needs to be rewritten for the new architecture (Phase 5)
// Temporary stub to allow compilation

use miette::Result;
use std::net::SocketAddr;
use tracing::info;

/// Placeholder for gRPC server - to be rewritten in Phase 5
pub async fn start_grpc_server(addr: SocketAddr) -> Result<()> {
    info!(%addr, "gRPC server not yet implemented - rewrite in progress");
    Err(miette::miette!(
        "gRPC server not yet implemented - needs rewrite for new architecture"
    ))
}

#[cfg(feature = "quic")]
pub mod quic {
    use miette::Result;
    use std::net::SocketAddr;
    use tracing::info;

    /// Placeholder for QUIC endpoint - to be rewritten in Phase 5
    pub async fn start_quic_endpoint(addr: SocketAddr) -> Result<()> {
        info!(%addr, "QUIC endpoint not yet implemented - rewrite in progress");
        Err(miette::miette!(
            "QUIC endpoint not yet implemented - needs rewrite for new architecture"
        ))
    }
}
