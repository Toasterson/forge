use forged::{telemetry, transport, AppState, Settings};
use std::net::SocketAddr;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialize tracing
    if let Err(e) = telemetry::init_tracing("forged") {
        eprintln!("Failed to initialize telemetry: {e:?}");
    }

    info!("Starting Forged V2");

    // Load settings
    let settings = match Settings::load() {
        Ok(s) => s,
        Err(e) => {
            error!(error = ?e, "Failed to load settings");
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    };

    // Parse listen address
    let addr: SocketAddr = match settings.server.listen_addr.parse() {
        Ok(a) => a,
        Err(e) => {
            error!(
                addr = %settings.server.listen_addr,
                error = ?e,
                "Invalid listen address"
            );
            eprintln!("Invalid listen address: {}", settings.server.listen_addr);
            std::process::exit(1);
        }
    };

    // Initialize application state
    info!("Initializing application state");
    let app_state = match AppState::new(settings.clone()).await {
        Ok(state) => state,
        Err(e) => {
            error!(error = ?e, "Failed to initialize application state");
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    };

    // Create gRPC services
    let auth_service = transport::AuthServiceImpl::new(app_state.actor_repo.clone());

    let gate_service = transport::GateServiceImpl::new(
        app_state.gate_repo.clone(),
        app_state.component_repo.clone(),
    );

    let component_service = transport::ComponentServiceImpl::new(
        app_state.component_repo.clone(),
        app_state.source_archive_repo.clone(),
    );

    let build_service = transport::BuildServiceImpl::new(
        app_state.component_repo.clone(),
        app_state.source_archive_repo.clone(),
        app_state.blob_repo.clone(),
    );

    // Set up graceful shutdown
    let app_state_for_shutdown = app_state.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
        info!("Received shutdown signal");
        if let Err(e) = app_state_for_shutdown.shutdown().await {
            error!(error = ?e, "Error during shutdown");
        }
    });

    // Start gRPC server
    info!(addr = %addr, "Starting gRPC server");
    if let Err(e) = transport::start_grpc_server(
        addr,
        auth_service,
        gate_service,
        component_service,
        build_service,
    )
    .await
    {
        error!(error = ?e, "gRPC server exited with error");
        eprintln!("gRPC server error: {:?}", e);
        std::process::exit(1);
    }

    info!("Server shutdown complete");
}
