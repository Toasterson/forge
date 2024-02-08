use async_graphql::{Context, Object, Result};

use crate::{
    graphql::types::{PaginationInput, Publisher},
    prisma, SharedState,
};

#[derive(Default)]
pub struct PublisherQuery;

#[Object]
impl PublisherQuery {
    async fn get_publishers(
        &self,
        ctx: &Context<'_>,
        pagination: Option<PaginationInput>,
    ) -> Result<Vec<Publisher>> {
        let database = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let pagination = pagination.unwrap_or_default();
        let mut query = database
            .publisher()
            .find_many(vec![])
            .take(pagination.limit);

        if let Some(cursor) = pagination.cursor {
            query = query.cursor(prisma::publisher::id::equals(cursor));
        }

        let publishers = query.exec().await?;

        Ok(publishers
            .into_iter()
            .map(|p| Publisher {
                id: p.id,
                name: p.name,
            })
            .collect())
    }
}
