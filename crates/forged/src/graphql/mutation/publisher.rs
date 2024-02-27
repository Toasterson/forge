use crate::{graphql::types::Publisher, SharedState};
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
        let database = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let publisher = database
            .publisher()
            .create(input.name, vec![])
            .exec()
            .await?;
        Ok(Publisher {
            id: publisher.id,
            name: publisher.name,
        })
    }
}
