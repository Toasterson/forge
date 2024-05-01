pub mod actor;
pub mod auth;
pub mod component;
pub mod gate;
pub mod publisher;

use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use crate::AppState;

pub fn get_v1_router() -> Router<AppState> {
    Router::new()
        .nest("/actors", actor::get_router())
        .nest("/components", component::get_router())
        .nest("/publishers", publisher::get_router())
        .nest("/gates", gate::get_router())
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct PaginationInput {
    pub cursor: Option<String>,
    pub limit: i64,
}

impl Default for PaginationInput {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 20,
        }
    }
}
