use async_graphql::{Context, Object, Result};

use crate::{AppState, SharedState};

#[derive(Default)]
pub struct PublisherQuery;

#[Object]
impl PublisherQuery {
    async fn get_publishers(&self, ctx: &Context<'_>) -> Result<Vec<String>> {
        let state = ctx.data_unchecked::<SharedState>();
        Ok(vec![])
    }
}
