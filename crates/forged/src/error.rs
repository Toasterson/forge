use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum ForgedError {
    #[error("Configuration error: {0}")]
    #[diagnostic(
        code(ips::config_error),
        help("check your forged configuration and environment variables")
    )]
    Config(String),

    #[error("Validation error: {0}")]
    #[diagnostic(code(ips::validation_error))]
    Validation(String),

    #[error("Authentication error: {0}")]
    #[diagnostic(code(ips::auth_error))]
    Auth(String),

    #[error("Storage error")]
    #[diagnostic(code(ips::storage_error))]
    Storage(#[source] anyhow::Error),

    #[error("Transport error")]
    #[diagnostic(code(ips::transport_error))]
    Transport(#[source] anyhow::Error),

    #[error("Telemetry error")]
    #[diagnostic(code(ips::telemetry_error))]
    Telemetry(#[source] anyhow::Error),
}

pub type Result<T, E = ForgedError> = std::result::Result<T, E>;
