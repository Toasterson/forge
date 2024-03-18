use async_graphql::{InputObject, scalar, SimpleObject};
use serde::{Deserialize, Serialize};
use component::{PackageMeta, Recipe};

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
    pub anitya_id: String,
    pub repology_id: String,
    pub project_url: String,
    pub gate: Gate,
    pub data: ComponentData,
}