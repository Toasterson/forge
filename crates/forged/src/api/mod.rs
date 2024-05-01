use crate::AppState;
use axum::Router;

mod auth;
pub mod v1;

pub fn get_api_router() -> Router<AppState> {
    Router::new().nest("/v1", v1::get_v1_router())
}
