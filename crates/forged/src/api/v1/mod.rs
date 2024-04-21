mod auth;
mod gate;
mod publisher;
mod component;

use axum::Router;
use serde::Deserialize;
use crate::SharedState;

pub fn get_v1_router() -> Router<SharedState> {
    Router::new()
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