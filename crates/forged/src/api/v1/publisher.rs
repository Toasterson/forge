use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::auth::Authentication;
use crate::api::v1::PaginationInput;
use crate::{prisma, AppState, Result};

pub fn get_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_publishers))
        .route("/", post(create_publisher))
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Publisher {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct CreatePublisherInput {
    pub name: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/publishers/",
    request_body = CreatePublisherInput,
    responses (
        (status = 200, description = "Successfully got the Publisher", body = Publisher),
        (status = 401, description = "Unauthorized to access the API", body = ApiError, example = json!(crate::ApiError::Unauthorized)),
        (status = 404, description = "Publisher not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn create_publisher(
    State(state): State<AppState>,
    Authentication { .. }: Authentication,
    Json(request): Json<CreatePublisherInput>,
) -> Result<Json<Publisher>> {
    let publisher = state
        .prisma
        .lock()
        .await
        .publisher()
        .create(request.name, vec![])
        .exec()
        .await?;
    Ok(Json(Publisher {
        id: publisher.id,
        name: publisher.name,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/publishers/",
    request_body = Option<PaginationInput>,
    responses (
        (status = 200, description = "Successfully got the Publishers", body = Vec<Publisher>),
    )
)]
async fn list_publishers(
    State(state): State<AppState>,
    Json(pagination): Json<Option<PaginationInput>>,
) -> Result<Json<Vec<Publisher>>> {
    let db_client = state.prisma.lock().await;
    let pagination = pagination.unwrap_or_default();
    let mut query = db_client
        .publisher()
        .find_many(vec![])
        .take(pagination.limit);

    if let Some(cursor) = pagination.cursor {
        query = query.cursor(prisma::publisher::id::equals(cursor));
    }

    let publishers = query.exec().await?;

    Ok(Json(
        publishers
            .into_iter()
            .map(|p| Publisher {
                id: p.id,
                name: p.name,
            })
            .collect(),
    ))
}
