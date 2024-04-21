use axum::Router;
use crate::SharedState;

mod v1;

pub fn get_api_router() -> Router<SharedState> {
    Router::new()
        .nest("/v1", v1::get_v1_router())
}