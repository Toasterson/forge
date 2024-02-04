use async_graphql::SimpleObject;

#[derive(Debug, SimpleObject)]
pub struct Publisher {
    pub name: String,
}
