use async_graphql::{Context, EmptySubscription, InputObject, Object, Schema, Result};
use sea_orm::*;
use uuid::Uuid;
use crate::entity::{*, prelude::*};

pub type ForgeSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn source_repo(&self, ctx: &Context<'_>) -> Result<Vec<SourceRepoObject>> {
        let db = ctx.data::<DatabaseConnection>()?;
        let repos = SourceRepo::find().all(db).await?;
        Ok(repos.into_iter().map(|r| SourceRepoObject(r)).collect())
    }
}


pub struct MutationRoot;

#[derive(InputObject)]
struct CreateRepoInput {
    name: String,
    url: String,
}

struct SourceRepoObject(source_repo::Model);

#[Object]
impl SourceRepoObject {
    async fn id(&self) -> String {
        self.0.id.hyphenated().to_string()
    }

    async fn name(&self) -> String {
        self.0.name.clone()
    }

    async fn url(&self) -> String {
        self.0.url.clone()
    }
}
#[Object]
impl MutationRoot {
    async fn create_source_repo(&self, ctx: &Context<'_>, input: CreateRepoInput) -> Result<SourceRepoObject> {
        let db = ctx.data::<DatabaseConnection>()?;
        let new_repo = source_repo::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            name: ActiveValue::Set(input.name),
            url: ActiveValue::Set(input.url),
        };
        let repo = new_repo.insert(db).await?;
        Ok(SourceRepoObject(repo))
    }
}