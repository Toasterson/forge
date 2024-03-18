mod gate;
mod publisher;
mod component;

pub use publisher::PublisherMutation;

use self::gate::GateMutation;

// Add your other ones here to create a unified Query object
// e.x. Query(PostQuery, OtherQuery, OtherOtherQuery)
#[derive(async_graphql::MergedObject, Default)]
pub struct MutationRoot(PublisherMutation, GateMutation);
