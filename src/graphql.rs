use async_graphql::{Context, Enum, InputObject, Object, Result};
use uuid::Uuid;

pub struct QueryRoot;

#[Object]
impl QueryRoot {}

pub struct MutationRoot;

#[Object]
impl MutationRoot {}
