use forged::services::BuildReportConsumer;
use forged::{telemetry, transport, AppState, Settings};
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
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

    // Declare AMQP topology
    if let Err(e) = app_state.build_dispatch.declare_topology().await {
        error!(error = ?e, "Failed to declare AMQP topology");
        eprintln!("AMQP topology error: {:?}", e);
        std::process::exit(1);
    }

    // Create gRPC services
    let auth_service = transport::AuthServiceImpl::new(app_state.auth.clone());

    let gate_service = transport::GateServiceImpl::new(
        app_state.gate_repo.clone(),
        app_state.component_repo.clone(),
        app_state.rbac.clone(),
    );

    let component_service = transport::ComponentServiceImpl::new(
        app_state.component_repo.clone(),
        app_state.source_archive_repo.clone(),
        app_state.rbac.clone(),
    );

    let build_service = transport::BuildServiceImpl::new(
        app_state.component_repo.clone(),
        app_state.source_archive_repo.clone(),
        app_state.blob_repo.clone(),
        app_state.rbac.clone(),
        app_state.build_dispatch.clone(),
    );

    // Set up graceful shutdown with cancellation token
    let cancel = CancellationToken::new();
    let app_state_for_shutdown = app_state.clone();
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
        info!("Received shutdown signal");
        cancel_for_signal.cancel();
        if let Err(e) = app_state_for_shutdown.shutdown().await {
            error!(error = ?e, "Error during shutdown");
        }
    });

    // Spawn build report consumer
    let consumer = BuildReportConsumer::new(
        app_state.build_job_repo.clone(),
        app_state.amqp_pool.clone(),
        app_state.settings.amqp.clone(),
        cancel.clone(),
    );
    tokio::spawn(async move {
        if let Err(e) = consumer.run().await {
            error!(error = ?e, "Build report consumer exited with error");
        }
    });

    // Start gRPC server with auth middleware
    info!(addr = %addr, "Starting gRPC server");
    if let Err(e) = transport::start_grpc_server(
        addr,
        auth_service,
        gate_service,
        component_service,
        build_service,
        app_state.oidc.clone(),
        app_state.actor_repo.clone(),
    )
    .await
    {
        error!(error = ?e, "gRPC server exited with error");
        eprintln!("gRPC server error: {:?}", e);
        std::process::exit(1);
    }

    info!("Server shutdown complete");
}
