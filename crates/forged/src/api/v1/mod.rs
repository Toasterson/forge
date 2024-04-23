mod auth;
mod component;
mod gate;
mod publisher;
mod actor;

use crate::SharedState;
use axum::Router;
use serde::Deserialize;

pub fn get_v1_router() -> Router<SharedState> {
    Router::new()
        .nest("/actors", actor::get_router())
        .nest("/components", component::get_router())
        .nest("/publishers", publisher::get_router())
        .nest("/gates", gate::get_router())
        .nest("/auth", auth::get_router())
}

#[derive(Deserialize, Debug)]
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
