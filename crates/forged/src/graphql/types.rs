use async_graphql::{InputObject, SimpleObject};

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
    pub r#ref: String,
    pub branch: String,
    pub transforms: Vec<String>,
}
