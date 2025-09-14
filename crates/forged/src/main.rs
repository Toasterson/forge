use forged::settings::Settings;
use forged::{telemetry, transport};
use std::net::SocketAddr;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    if let Err(e) = telemetry::init_tracing("forged") {
        eprintln!("failed to init telemetry: {e:?}");
    }

    // Load Settings and compute listen address with env fallback
    let settings = match Settings::load() {
        Ok(s) => s,
        Err(e) => {
            error!(error=?e, "failed to load settings; refusing to start");
            eprintln!("failed to load settings: {e:?}");
            std::process::exit(1);
        }
    };
    let addr_str = settings
        .server
        .listen_addr
        .clone()
        .or_else(|| std::env::var("FORGED_ADDR").ok())
        .unwrap_or_else(|| "127.0.0.1:50051".to_string());
    let addr: SocketAddr = addr_str
        .parse()
        .unwrap_or_else(|_| "127.0.0.1:50051".parse().expect("hardcoded addr parses"));

    // Optionally start QUIC endpoint if enabled
    #[cfg(feature = "quic")]
    if let Ok(quic_addr) = std::env::var("FORGED_QUIC_ADDR") {
        if let Ok(quic_sock) = quic_addr.parse() {
            tokio::spawn(async move {
                if let Err(e) = transport::quic::start_quic_endpoint(quic_sock).await {
                    error!(error=?e, "quic endpoint exited with error");
                }
            });
            info!(addr=%quic_addr, "started QUIC endpoint");
        } else {
            error!(addr=%quic_addr, "invalid FORGED_QUIC_ADDR");
        }
    } else {
        info!("FORGED_QUIC_ADDR not set; QUIC endpoint disabled");
    }

    // Start gRPC server (TCP)
    if let Err(e) = transport::start_grpc_server(addr).await {
        error!(error=?e, "grpc server exited with error");
    }
}
