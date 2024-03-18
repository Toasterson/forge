mod gate;
mod publisher;
mod component;

pub use publisher::PublisherQuery;
use crate::graphql::query::component::ComponentQuery;
use crate::graphql::query::gate::GateQuery;

// Add your other ones here to create a unified Query object
// e.x. Query(PostQuery, OtherQuery, OtherOtherQuery)
#[derive(async_graphql::MergedObject, Default)]
pub struct QueryRoot(PublisherQuery, GateQuery, ComponentQuery);
