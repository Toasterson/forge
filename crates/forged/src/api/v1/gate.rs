use crate::prisma::gate::{SetParam, WhereParam};
use crate::{prisma, Error, Result, SharedState};
use axum::extract::{Path, State};
use axum::routing::{post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::error;

pub fn get_router() -> Router<SharedState> {
    Router::new()
        .route("/get", post(get_gate))
        .route("/list", post(list_gates))
        .route("/", post(create_gate))
        .route("/:id", put(update_gate))
}

#[derive(Deserialize, Debug, Serialize)]
pub struct GateSearchRequest {
    publisher: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Gate {
    pub id: String,
    pub name: String,
    pub version: String,
    pub branch: String,
    pub publisher: String,
    pub transforms: Vec<String>,
}

async fn get_gate(
    State(state): State<SharedState>,
    Json(request): Json<GateSearchRequest>,
) -> Result<Json<Gate>> {
    let gate = state
        .lock()
        .await
        .prisma
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

#[derive(Deserialize, Debug, Serialize)]
pub struct GateListRequest {
    publisher: Option<String>,
}
async fn list_gates(
    State(state): State<SharedState>,
    Json(request): Json<GateListRequest>,
) -> Result<Json<Vec<Gate>>> {
    let mut filter: Vec<WhereParam> = vec![];
    if let Some(publisher) = request.publisher {
        filter.push(prisma::gate::publisher::is(vec![
            prisma::publisher::name::equals(publisher),
        ]));
    }

    let gates = state
        .lock()
        .await
        .prisma
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGateInput {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub branch: String,
    pub transforms: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateGateInput {
    pub name: Option<String>,
    pub version: Option<String>,
    pub branch: Option<String>,
    pub transforms: Option<Vec<String>>,
}

async fn create_gate(
    State(state): State<SharedState>,
    Json(request): Json<CreateGateInput>,
) -> Result<Json<Gate>> {
    let encoded_transforms = serde_json::to_value(request.transforms)?;
    if state
        .lock()
        .await
        .prisma
        .publisher()
        .find_unique(prisma::publisher::UniqueWhereParam::NameEquals(
            request.publisher.clone(),
        ))
        .exec()
        .await?
        .is_none()
    {
        state
            .lock()
            .await
            .prisma
            .publisher()
            .create(request.publisher.clone(), vec![])
            .exec()
            .await?;
    }

    let gate = state
        .lock()
        .await
        .prisma
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

async fn update_gate(
    State(state): State<SharedState>,
    Path(id): Path<String>,
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
        .lock()
        .await
        .prisma
        .gate()
        .update(prisma::gate::id::equals(id), updates)
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
