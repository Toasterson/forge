use forged::acme;
use forged::services::BuildReportConsumer;
use forged::settings::TlsMode;
use forged::transport::grpc::HealthCheckDeps;
use forged::{telemetry, transport, AppState, Settings};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Install rustls crypto provider before any TLS/ACME code runs
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    if let Err(e) = telemetry::init_tracing("forged") {
        eprintln!("Failed to initialize telemetry: {e:?}");
    }

    info!("Starting Forged V2");

    let settings = match Settings::load() {
        Ok(s) => s,
        Err(e) => {
            error!(error = ?e, "Failed to load settings");
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    };

    let addr: SocketAddr = match settings.server.listen_addr.parse() {
        Ok(a) => a,
        Err(e) => {
            error!(addr = %settings.server.listen_addr, error = ?e, "Invalid listen address");
            std::process::exit(1);
        }
    };

    // Set up graceful shutdown
    let cancel = CancellationToken::new();
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
        info!("Received shutdown signal");
        cancel_for_signal.cancel();
    });

    // Handle ACME certificate acquisition before starting gRPC
    if settings.tls.mode == TlsMode::Acme {
        let acme_manager = match acme::AcmeManager::new(settings.tls.acme.clone()) {
            Ok(m) => Arc::new(m),
            Err(e) => {
                error!(error = ?e, "Failed to initialize ACME manager");
                std::process::exit(1);
            }
        };

        if settings.tls.acme.challenge_type == "http-01" {
            let http_addr: SocketAddr = match settings.tls.acme.http_listen_addr.parse() {
                Ok(a) => a,
                Err(e) => {
                    error!(error = ?e, "Invalid ACME HTTP listen address");
                    std::process::exit(1);
                }
            };

            // Pre-flight: verify we can bind the HTTP-01 challenge port before
            // contacting Let's Encrypt. This prevents hammering the ACME server
            // with orders that will inevitably fail validation.
            match tokio::net::TcpListener::bind(http_addr).await {
                Ok(listener) => {
                    drop(listener); // Release for the actual challenge server
                    info!(%http_addr, "HTTP-01 challenge port is available");
                }
                Err(e) => {
                    error!(
                        %http_addr,
                        error = %e,
                        "Cannot bind HTTP-01 challenge port.\n\
                         On illumos, ensure the SMF manifest grants net_privaddr privilege.\n\
                         Check that no other service is using the port."
                    );
                    std::process::exit(1);
                }
            }

            let tokens = acme_manager.challenge_tokens();
            let cancel_http = cancel.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    acme::start_http01_challenge_server(http_addr, tokens, cancel_http).await
                {
                    error!(error = ?e, "HTTP-01 challenge server error");
                }
            });

            // Give the challenge server a moment to start, then verify it responds
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            match reqwest::get(format!("http://127.0.0.1:{}/self-check", http_addr.port())).await {
                Ok(resp) if resp.status() == 404 => {
                    info!("HTTP-01 challenge server self-check passed");
                }
                Ok(resp) => {
                    info!(status = %resp.status(), "HTTP-01 challenge server is responding");
                }
                Err(e) => {
                    error!(
                        error = %e,
                        "HTTP-01 challenge server self-check failed.\n\
                         The server bound the port but is not responding to HTTP requests."
                    );
                    std::process::exit(1);
                }
            }
        }

        if let Err(e) = acme_manager.ensure_certificate().await {
            error!(error = ?e, "Failed to acquire ACME certificate");
            std::process::exit(1);
        }

        acme::spawn_renewal_task(acme_manager, cancel.clone());
    }

    // Initialize application state
    let app_state = match AppState::new(settings.clone()).await {
        Ok(state) => state,
        Err(e) => {
            error!(error = ?e, "Failed to initialize application state");
            std::process::exit(1);
        }
    };

    if let Err(e) = app_state.build_dispatch.declare_topology().await {
        error!(error = ?e, "Failed to declare AMQP topology");
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
    )
    .with_max_upload_size(settings.server.max_upload_size);
    let build_service = transport::BuildServiceImpl::new(
        app_state.component_repo.clone(),
        app_state.source_archive_repo.clone(),
        app_state.blob_repo.clone(),
        app_state.rbac.clone(),
        app_state.build_dispatch.clone(),
    );

    // Spawn build report consumer
    let consumer = BuildReportConsumer::new(
        app_state.build_job_repo.clone(),
        app_state.amqp_pool.clone(),
        app_state.settings.amqp.clone(),
        cancel.clone(),
    );
    let app_state_for_shutdown = app_state.clone();
    tokio::spawn(async move {
        if let Err(e) = consumer.run().await {
            error!(error = ?e, "Build report consumer error");
        }
    });

    // Graceful shutdown handler
    let cancel_for_shutdown = cancel.clone();
    tokio::spawn(async move {
        cancel_for_shutdown.cancelled().await;
        if let Err(e) = app_state_for_shutdown.shutdown().await {
            error!(error = ?e, "Shutdown error");
        }
    });

    // Health check dependencies
    let health_deps = HealthCheckDeps {
        db: app_state.db.clone(),
        seaweedfs_url: settings.seaweedfs.master_url.clone(),
        cancel: cancel.clone(),
    };

    // Start gRPC server
    info!(addr = %addr, "Starting gRPC server");
    if let Err(e) = transport::start_grpc_server(
        addr,
        auth_service,
        gate_service,
        component_service,
        build_service,
        app_state.oidc.clone(),
        app_state.actor_repo.clone(),
        &settings.tls,
        Some(health_deps),
        settings.server.rate_limit_requests as usize,
    )
    .await
    {
        error!(error = ?e, "gRPC server error");
        std::process::exit(1);
    }

    info!("Server shutdown complete");
}
