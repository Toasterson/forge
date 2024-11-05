#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use crate::models::_entities::gates::{ActiveModel, Column, Entity, Model};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddParams {
    pub name: String,
    pub version: String,
    pub id: Uuid,
    pub branch: String,
    pub transforms: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateParams {
    pub version: Option<String>,
    pub branch: Option<String>,
    pub transforms: Option<Vec<Value>>,
}

impl UpdateParams {
    fn update_model(&self, item: &mut ActiveModel) {
        if let Some(version) = &self.version {
            item.version = ActiveValue::Set(version.clone());
        }
        if let Some(branch) = &self.branch {
            item.branch = ActiveValue::Set(branch.clone());
        }
        if let Some(transforms) = &self.transforms {
            if transforms.is_empty() {
                item.transforms = ActiveValue::NotSet;
            } else {
                let tr = serde_json::to_value(transforms.clone()).unwrap();
                item.transforms = ActiveValue::Set(Some(tr));
            }
        }
    }
}

pub async fn list(State(ctx): State<AppContext>) -> Result<Response> {
    let gates = Entity::find().all(&ctx.db).await?;
    format::json(&gates)
}

async fn load_gate(ctx: &AppContext, name: &str) -> Result<Model> {
    let gate = Entity::find()
        .filter(Column::Name.contains(name))
        .one(&ctx.db)
        .await?;
    gate.ok_or_else(|| Error::NotFound)
}

pub async fn add(State(ctx): State<AppContext>, Json(params): Json<AddParams>) -> Result<Response> {
    let gate = ActiveModel {
        pid: ActiveValue::Set(params.id),
        name: ActiveValue::Set(params.name),
        version: ActiveValue::Set(params.version),
        branch: ActiveValue::Set(params.branch),
        transforms: ActiveValue::Set(params.transforms),
        ..Default::default()
    };

    let gate = gate.insert(&ctx.db).await?;
    format::json(&gate)
}

pub async fn update(
    State(ctx): State<AppContext>,
    Path(name): Path<String>,
    Json(params): Json<UpdateParams>,
) -> Result<Response> {
    let gate = load_gate(&ctx, &name).await?;
    let mut gate = gate.into_active_model();
    params.update_model(&mut gate);
    let gate = gate.update(&ctx.db).await?;
    format::json(&gate)
}

pub async fn get_one(State(ctx): State<AppContext>, Path(name): Path<String>) -> Result<Response> {
    let gate = load_gate(&ctx, &name).await?;
    format::json(&gate)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("gates/")
        .add("/", get(list))
        .add("/", post(add))
        .add("/:name", post(update))
        .add("/:name", get(get_one))
}
