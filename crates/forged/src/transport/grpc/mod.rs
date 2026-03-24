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
use crate::settings::{TlsConfig, TlsMode};
use miette::{IntoDiagnostic, Result};
use sea_orm::DatabaseConnection;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic_health::ServingStatus;
use tower::limit::ConcurrencyLimitLayer;
use tracing::info;

/// Dependencies for background health checking.
pub struct HealthCheckDeps {
    pub db: Arc<DatabaseConnection>,
    pub seaweedfs_url: String,
    pub cancel: tokio_util::sync::CancellationToken,
}

/// Start the gRPC server with all services, auth middleware, TLS, health, and concurrency limiting
#[allow(clippy::too_many_arguments)]
pub async fn start_server(
    addr: SocketAddr,
    auth_service: AuthServiceImpl,
    gate_service: GateServiceImpl,
    component_service: ComponentServiceImpl,
    build_service: BuildServiceImpl,
    oidc: Arc<OidcService>,
    actor_repo: Arc<ActorRepository>,
    tls_config: &TlsConfig,
    health_deps: Option<HealthCheckDeps>,
    max_concurrent_requests: usize,
) -> Result<()> {
    info!(%addr, "Starting gRPC server");

    let auth_layer = middleware::tower_auth::AuthLayer::new(oidc, actor_repo);
    let mut builder = Server::builder();

    match &tls_config.mode {
        TlsMode::Manual => {
            let cert = std::fs::read(&tls_config.manual.cert_file).into_diagnostic()?;
            let key = std::fs::read(&tls_config.manual.key_file).into_diagnostic()?;
            let identity = Identity::from_pem(cert, key);
            let mut tls = ServerTlsConfig::new().identity(identity);
            if let Some(ca_path) = &tls_config.manual.client_ca_file {
                let ca = std::fs::read(ca_path).into_diagnostic()?;
                tls = tls.client_ca_root(tonic::transport::Certificate::from_pem(ca));
            }
            builder = builder.tls_config(tls).into_diagnostic()?;
            info!("TLS enabled (manual certificate)");
        }
        TlsMode::Acme => {
            let cache_dir = std::path::Path::new(&tls_config.acme.cache_dir);
            let cert = std::fs::read(cache_dir.join("cert.pem")).into_diagnostic()?;
            let key = std::fs::read(cache_dir.join("key.pem")).into_diagnostic()?;
            let identity = Identity::from_pem(cert, key);
            builder = builder
                .tls_config(ServerTlsConfig::new().identity(identity))
                .into_diagnostic()?;
            info!(domains = ?tls_config.acme.domains, "TLS enabled (ACME)");
        }
        TlsMode::None => {
            info!("TLS disabled (plaintext mode)");
        }
    }

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_service_status("", ServingStatus::Serving)
        .await;

    if let Some(deps) = health_deps {
        tokio::spawn(async move {
            let interval = tokio::time::Duration::from_secs(10);
            loop {
                tokio::select! {
                    _ = deps.cancel.cancelled() => break,
                    _ = tokio::time::sleep(interval) => {
                        let db_ok = deps.db.ping().await.is_ok();
                        let sw_ok = reqwest::get(format!("{}/cluster/status", deps.seaweedfs_url))
                            .await.map(|r| r.status().is_success()).unwrap_or(false);
                        let status = if db_ok && sw_ok { ServingStatus::Serving } else {
                            if !db_ok { tracing::warn!("Health: PostgreSQL not responding"); }
                            if !sw_ok { tracing::warn!("Health: SeaweedFS not responding"); }
                            ServingStatus::NotServing
                        };
                        health_reporter.set_service_status("", status).await;
                    }
                }
            }
        });
    }

    let concurrency_limit = ConcurrencyLimitLayer::new(max_concurrent_requests);

    builder
        .concurrency_limit_per_connection(256)
        .layer(concurrency_limit)
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
        .add_service(health_service)
        .serve(addr)
        .await
        .into_diagnostic()?;

    Ok(())
}
