mod publisher;

pub use publisher::PublisherMutation;

// Add your other ones here to create a unified Query object
// e.x. Query(PostQuery, OtherQuery, OtherOtherQuery)
#[derive(async_graphql::MergedObject, Default)]
pub struct MutationRoot(PublisherMutation);
