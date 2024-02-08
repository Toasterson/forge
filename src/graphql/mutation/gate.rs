use async_graphql::{Context, InputObject, Object, Result};

use crate::{
    graphql::types::Gate,
    prisma::{self, gate::SetParam},
    SharedState,
};

#[derive(Debug, InputObject)]
pub struct CreateGateInput {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub r#ref: String,
    pub branch: String,
    pub transforms: Vec<String>,
}

#[derive(Debug, InputObject)]
pub struct UpdateGateInput {
    pub id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub r#ref: Option<String>,
    pub branch: Option<String>,
    pub transforms: Option<Vec<String>>,
}

#[derive(Default)]
pub struct GateMutation;

#[Object]
impl GateMutation {
    async fn create_gate(&self, ctx: &Context<'_>, input: CreateGateInput) -> Result<Gate> {
        let database = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let encoded_transforms = serde_json::to_value(input.transforms)?;
        let gate = database
            .gate()
            .create(
                input.name,
                input.version,
                input.r#ref,
                input.branch,
                prisma::publisher::name::equals(input.publisher),
                encoded_transforms,
                vec![],
            )
            .exec()
            .await?;

        let transforms: Vec<String> = serde_json::from_value(gate.transforms)?;
        Ok(Gate {
            id: gate.id,
            name: gate.name,
            version: gate.version,
            r#ref: gate.r#ref,
            branch: gate.branch,
            transforms,
        })
    }

    async fn update_gate(&self, ctx: &Context<'_>, input: UpdateGateInput) -> Result<Gate> {
        let database = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let mut updates: Vec<SetParam> = vec![];

        if let Some(name) = input.name {
            updates.push(prisma::gate::name::set(name));
        }

        if let Some(version) = input.version {
            updates.push(prisma::gate::version::set(version));
        }

        if let Some(r#ref) = input.r#ref {
            updates.push(prisma::gate::r#ref::set(r#ref));
        }

        if let Some(branch) = input.branch {
            updates.push(prisma::gate::branch::set(branch));
        }

        if let Some(transforms) = input.transforms {
            let encoded_transforms = serde_json::to_value(&transforms)?;
            updates.push(prisma::gate::transforms::set(encoded_transforms));
        }

        let gate = database
            .gate()
            .update(prisma::gate::id::equals(input.id), updates)
            .exec()
            .await?;

        let transforms: Vec<String> = serde_json::from_value(gate.transforms)?;
        Ok(Gate {
            id: gate.id,
            name: gate.name,
            version: gate.version,
            r#ref: gate.r#ref,
            branch: gate.branch,
            transforms,
        })
    }
}
