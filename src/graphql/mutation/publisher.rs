use crate::graphql::types::Publisher;
use async_graphql::{Context, InputObject, Object, Result};
#[derive(Debug, InputObject)]
pub struct CreatePublisherInput {
    pub name: String,
}

#[derive(Default)]
pub struct PublisherMutation;

#[Object]
impl PublisherMutation {
    async fn create_publisher(
        &self,
        ctx: &Context<'_>,
        input: CreatePublisherInput,
    ) -> Result<Publisher> {
        //let state: AppState = ctx.
        todo!()
    }
}
