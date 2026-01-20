pub mod app_state;
pub mod entities;
pub mod repositories;
pub mod services;
pub mod settings;
pub mod storage;
pub mod telemetry;
pub mod transport;
pub mod types;

// Re-exports for convenience
pub use app_state::AppState;
pub use settings::Settings;

// Legacy modules (to be removed)
#[allow(dead_code)]
pub mod api;
#[allow(dead_code)]
pub mod auth;
#[allow(dead_code)]
pub mod component;
#[allow(dead_code)]
pub mod error;
#[allow(dead_code)]
pub mod gate;
#[allow(dead_code)]
pub mod rbac;
