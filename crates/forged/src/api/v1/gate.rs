use crate::prisma::gate::{SetParam, WhereParam};
use crate::{prisma, Error, Result, AppState};
use axum::extract::{Path, State};
use axum::routing::{post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::error;
use utoipa::ToSchema;
use uuid::Uuid;
use crate::api::auth::Authentication;

pub fn get_router() -> Router<AppState> {
    Router::new()
        .route("/get", post(get_gate))
        .route("/list", post(list_gates))
        .route("/", post(create_gate))
        .route("/:id", put(update_gate))
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GateSearchRequest {
    publisher: String,
    name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Gate {
    pub id: String,
    pub name: String,
    pub version: String,
    pub branch: String,
    pub publisher: String,
    pub transforms: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/gates/get",
    request_body = GateSearchRequest,
    responses (
        (status = 200, description = "Successfully retrieved gate info", body = Gate),
        (status = 404, description = "Gate not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn get_gate(
    State(state): State<AppState>,
    Json(request): Json<GateSearchRequest>,
) -> Result<Json<Gate>> {
    let gate = state
        .prisma
        .lock()
        .await
        .gate()
        .find_first(vec![
            prisma::gate::publisher::is(vec![prisma::publisher::name::equals(
                request.publisher.clone(),
            )]),
            prisma::gate::name::equals(request.name.clone()),
        ])
        .with(prisma::gate::publisher::fetch())
        .exec()
        .await?
        .ok_or(Error::NotFound(format!(
            "gate with publisher {0} and name {1}",
            request.publisher, request.name
        )))?;

    let transforms = serde_json::from_value(gate.transforms)?;
    Ok(Json(Gate {
        id: gate.id,
        name: gate.name,
        version: gate.version,
        branch: gate.branch,
        publisher: gate.publisher.unwrap().name,
        transforms,
    }))
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GateListRequest {
    publisher: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/gates/list",
    request_body = GateListRequest,
    responses (
        (status = 200, description = "Successfully retrieved gate info", body = Vec<Gate>),
        (status = 404, description = "Gate not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn list_gates(
    State(state): State<AppState>,
    Json(request): Json<GateListRequest>,
) -> Result<Json<Vec<Gate>>> {
    let mut filter: Vec<WhereParam> = vec![];
    if let Some(publisher) = request.publisher {
        filter.push(prisma::gate::publisher::is(vec![
            prisma::publisher::name::equals(publisher),
        ]));
    }

    let gates = state
        .prisma
        .lock()
        .await
        .gate()
        .find_many(filter)
        .with(prisma::gate::publisher::fetch())
        .exec()
        .await?;
    Ok(Json(gates
        .into_iter()
        .map(|g| {
            let transforms: Vec<String> = match serde_json::from_value(g.transforms) {
                Ok(v) => v,
                Err(e) => {
                    error!(error=e.to_string(), "Could not retrieve transforms for gate id: {0}: transforms are malformed", g.id);
                    vec![]
                }
            };
            Gate {
                id: g.id,
                name: g.name,
                version: g.version,
                branch: g.branch,
                publisher: g.publisher.unwrap().name,
                transforms,
            }
        })
        .collect()))
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct CreateGateInput {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub branch: String,
    pub transforms: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct UpdateGateInput {
    pub name: Option<String>,
    pub version: Option<String>,
    pub branch: Option<String>,
    pub transforms: Option<Vec<String>>,
}

#[utoipa::path(
    post,
    path = "/api/v1/gates/",
    request_body = CreateGateInput,
    responses (
        (status = 200, description = "Successfully retrieved gate info", body = Gate),
        (status = 401, description = "Unauthorized to access the API", body = ApiError, example = json!(crate::ApiError::Unauthorized)),
        (status = 404, description = "Gate not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn create_gate(
    State(state): State<AppState>,
    Authentication { .. }: Authentication,
    Json(request): Json<CreateGateInput>,
) -> Result<Json<Gate>> {
    let encoded_transforms = serde_json::to_value(request.transforms)?;
    if state
        .prisma
        .lock()
        .await
        .publisher()
        .find_unique(prisma::publisher::UniqueWhereParam::NameEquals(
            request.publisher.clone(),
        ))
        .exec()
        .await?
        .is_none()
    {
        state
            .prisma
            .lock()
            .await
            .publisher()
            .create(request.publisher.clone(), vec![])
            .exec()
            .await?;
    }

    let gate = state
        .prisma
        .lock()
        .await
        .gate()
        .create(
            request.name,
            request.version,
            request.branch,
            prisma::publisher::name::equals(request.publisher),
            encoded_transforms,
            vec![],
        )
        .with(prisma::gate::publisher::fetch())
        .exec()
        .await?;

    let transforms: Vec<String> = serde_json::from_value(gate.transforms)?;
    Ok(Json(Gate {
        id: gate.id,
        name: gate.name,
        version: gate.version,
        branch: gate.branch,
        publisher: gate.publisher.unwrap().name,
        transforms,
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/gates/{id}",
    request_body = UpdateGateInput,
    responses (
        (status = 200, description = "Successfully retrieved gate info", body = Gate),
        (status = 401, description = "Unauthorized to access the API", body = ApiError, example = json!(crate::ApiError::Unauthorized)),
        (status = 404, description = "Gate not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    ),
    params(
        ("id" = Uuid, Path, description = "Database id of the Gate to update"),
    )
)]
async fn update_gate(
    State(state): State<AppState>,
    Authentication { .. }: Authentication,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateGateInput>,
) -> Result<Json<Gate>> {
    let mut updates: Vec<SetParam> = vec![];

    if let Some(name) = request.name {
        updates.push(prisma::gate::name::set(name));
    }

    if let Some(version) = request.version {
        updates.push(prisma::gate::version::set(version));
    }

    if let Some(branch) = request.branch {
        updates.push(prisma::gate::branch::set(branch));
    }

    if let Some(transforms) = request.transforms {
        let encoded_transforms = serde_json::to_value(&transforms)?;
        updates.push(prisma::gate::transforms::set(encoded_transforms));
    }

    let gate = state
        .prisma
        .lock()
        .await
        .gate()
        .update(prisma::gate::id::equals(id.to_string()), updates)
        .with(prisma::gate::publisher::fetch())
        .exec()
        .await?;

    let transforms: Vec<String> = serde_json::from_value(gate.transforms)?;
    Ok(Json(Gate {
        id: gate.id,
        name: gate.name,
        version: gate.version,
        branch: gate.branch,
        publisher: gate.publisher.unwrap().name,
        transforms,
    }))
}
