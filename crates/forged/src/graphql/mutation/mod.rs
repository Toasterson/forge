mod component;
mod gate;
mod publisher;

use crate::graphql::mutation::component::ComponentMutation;
pub use publisher::PublisherMutation;

use self::gate::GateMutation;

// Add your other ones here to create a unified Query object
// e.x. Query(PostQuery, OtherQuery, OtherOtherQuery)
#[derive(async_graphql::MergedObject, Default)]
pub struct MutationRoot(PublisherMutation, GateMutation, ComponentMutation);
