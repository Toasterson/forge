use crate::prisma;
use async_graphql::{scalar, InputObject, Result, SimpleObject};
use component::{PackageMeta, Recipe};
use serde::{Deserialize, Serialize};

#[derive(InputObject)]
pub struct PaginationInput {
    pub cursor: Option<String>,
    #[graphql(default = 20)]
    pub limit: i64,
}

impl Default for PaginationInput {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 20,
        }
    }
}

#[derive(Debug, SimpleObject)]
pub struct Publisher {
    pub id: String,
    pub name: String,
}

#[derive(Debug, SimpleObject)]
pub struct Gate {
    pub id: String,
    pub name: String,
    pub version: String,
    pub branch: String,
    pub publisher: String,
    pub transforms: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentData {
    pub recipe: Recipe,
    pub packages: PackageMeta,
}
scalar!(ComponentData);
#[derive(Debug, SimpleObject)]
pub struct Component {
    pub name: String,
    pub version: String,
    pub revision: String,
    pub anitya_id: Option<String>,
    pub repology_id: Option<String>,
    pub project_url: String,
    pub gate_id: String,
    pub data: ComponentData,
}

pub fn component_from_database(component: prisma::component::Data) -> Result<Component> {
    let r = Component {
        name: component.name,
        version: component.version,
        gate_id: component.gate_id.to_string(),
        revision: component.revision,
        anitya_id: component.anitya_id.clone(),
        repology_id: component.repology_id.clone(),
        project_url: component.project_url,
        data: ComponentData {
            recipe: serde_json::from_value(component.recipe)?,
            packages: serde_json::from_value(component.packages)?,
        },
    };
    Ok(r)
}
