use async_graphql::{Context, InputObject, Object, Result};

use crate::{
    graphql::types::Gate,
    prisma::{self, component::SetParam},
    SharedState,
};