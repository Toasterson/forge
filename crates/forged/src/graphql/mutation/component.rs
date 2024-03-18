use async_graphql::{Context, InputObject, Object, Result};

use crate::{Error, prisma::{self}, SharedState};
use crate::graphql::types::{Component, component_from_database, ComponentData};

#[derive(Debug, InputObject)]
pub struct ComponentInput {
    pub name: String,
    pub version: String,
    pub revision: Option<String>,
    pub gate_id: String,
    pub data: ComponentData,
    pub anitya_id: Option<String>,
    pub repology_id: Option<String>,
    pub project_url: Option<String>,
}

#[derive(Default)]
pub struct ComponentMutation;

#[Object]
impl ComponentMutation {
    async fn create_component(&self, ctx: &Context<'_>, input: ComponentInput) -> Result<Component> {
        let db = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let encoded_recipe = serde_json::to_value(&input.data.recipe)?;
        let encoded_package_meta = serde_json::to_value(&input.data.packages)?;
        
        let component = db.component().create(
            input.data.recipe.name.clone(),
            input.data.recipe.version.clone().ok_or(Error::NoVersionFoundInRecipe(input.data.recipe.name.clone()))?,
            input.data.recipe.revision.clone().unwrap_or(String::from("0")),
            input.data.recipe.project_url.clone().ok_or(Error::NoProjectUrlFoundInRecipe(input.data.recipe.name.clone()))?,
            prisma::gate::UniqueWhereParam::IdEquals(input.gate_id),
            encoded_recipe,
            encoded_package_meta,
            vec![]
        ).exec().await?;
        
        component_from_database(component)
    }
}