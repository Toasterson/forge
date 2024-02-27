use async_graphql::{Context, Object, Result};
use tracing::error;

use crate::{
    graphql::types::Gate,
    prisma::{self, gate::WhereParam},
    Error, SharedState,
};

#[derive(Default)]
pub struct GateQuery;

#[Object]
impl GateQuery {
    async fn get_gate(&self, ctx: &Context<'_>, publisher: String, name: String) -> Result<Gate> {
        let database = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let gate = database
            .gate()
            .find_first(vec![])
            .exec()
            .await?
            .ok_or(Error::NotFound(format!(
                "gate with publisher {0} and name {1}",
                publisher, name
            )))?;

        let transforms = serde_json::from_value(gate.transforms)?;
        Ok(Gate {
            id: gate.id,
            name: gate.name,
            version: gate.version,
            r#ref: gate.r#ref,
            branch: gate.branch,
            transforms,
        })
    }

    async fn gates(&self, ctx: &Context<'_>, publisher: Option<String>) -> Result<Vec<Gate>> {
        let database = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let mut filter: Vec<WhereParam> = vec![];
        if let Some(publisher) = publisher {
            filter.push(prisma::gate::publisher::is(vec![
                prisma::publisher::name::equals(publisher),
            ]));
        }

        let gates = database.gate().find_many(filter).exec().await?;
        Ok(gates
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
                    r#ref: g.r#ref,
                    branch: g.branch,
                    transforms,
                }
            })
            .collect())
    }
}
