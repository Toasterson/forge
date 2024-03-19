use async_graphql::{Context, Object, Result};
use itertools::Itertools;

use crate::graphql::types::{component_from_database, Component};
use crate::{
    prisma::{self},
    SharedState,
};

#[derive(Default)]
pub struct ComponentQuery;

#[Object]
impl ComponentQuery {
    async fn get_component(
        &self,
        ctx: &Context<'_>,
        name: String,
        version: String,
        revision: String,
        gate_id: String,
    ) -> Result<Component> {
        let db = &ctx.data_unchecked::<SharedState>().lock().await.prisma;

        let component = db
            .component()
            .find_unique(
                prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                    name, version, revision, gate_id,
                ),
            )
            .exec()
            .await?;

        if let Some(component) = component {
            component_from_database(component)
        } else {
            Err(async_graphql::Error {
                message: String::from("cannot find any component matching this search"),
                source: None,
                extensions: None,
            })
        }
    }

    async fn components(
        &self,
        ctx: &Context<'_>,
        name: String,
        version: Option<String>,
        revision: Option<String>,
        gate_id: Option<String>,
    ) -> Result<Vec<Component>> {
        let db = &ctx.data_unchecked::<SharedState>().lock().await.prisma;

        let mut filter = vec![prisma::component::name::equals(name)];

        if let Some(version) = version {
            filter.push(prisma::component::version::equals(version))
        }

        if let Some(revision) = revision {
            filter.push(prisma::component::revision::equals(revision))
        }

        if let Some(gate_id) = gate_id {
            filter.push(prisma::component::gate_id::equals(gate_id))
        }

        let components = db.component().find_many(filter).exec().await?;

        components
            .into_iter()
            .map(|component| component_from_database(component))
            .try_collect()
    }
}
