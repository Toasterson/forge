use axum::{Json, Router};
use axum::extract::State;
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use crate::{prisma, Result, SharedState};
use crate::api::v1::PaginationInput;

pub fn get_router() -> Router<SharedState> {
    Router::new()
        .route("/", get(list_publishers))
        .route("/", post(create_publisher))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Publisher {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePublisherInput {
    pub name: String,
}

async fn create_publisher(
    State(state): State<SharedState>,
    Json(request): Json<CreatePublisherInput>,
) -> Result<Json<Publisher>> {
    let publisher = state.lock().await.prisma
        .publisher()
        .create(request.name, vec![])
        .exec()
        .await?;
    Ok(Json(Publisher {
        id: publisher.id,
        name: publisher.name,
    }))
}

async fn list_publishers(
    State(state): State<SharedState>,
    Json(pagination): Json<Option<PaginationInput>>,
) -> Result<Json<Vec<Publisher>>> {
    let state = state.lock().await;
    let pagination = pagination.unwrap_or_default();
    let mut query = state.prisma
        .publisher()
        .find_many(vec![])
        .take(pagination.limit);

    if let Some(cursor) = pagination.cursor {
        query = query.cursor(prisma::publisher::id::equals(cursor));
    }

    let publishers = query.exec().await?;

    Ok(Json(publishers
        .into_iter()
        .map(|p| Publisher {
            id: p.id,
            name: p.name,
        })
        .collect()))
}