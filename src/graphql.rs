use async_graphql::{Context, InputObject, Object, Result, Enum};
use sea_orm::*;
use uuid::Uuid;
use crate::entity::{*, prelude::*};
use crate::entity::sea_orm_active_enums::RecipeKind;

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
    repo_kind: Option<SourceRepoKind>,
    recipes_kind: RecipesKind,
}

#[derive(strum::Display, Enum, Copy, Clone, Eq, PartialEq)]
enum RecipesKind {
    OpenIndianaUserland,
    Forge,
}

impl From<RecipeKind> for RecipesKind {
    fn from(value: RecipeKind) -> Self {
        match value {
            RecipeKind::Forge => RecipesKind::Forge,
            RecipeKind::OpenIndianaUserland => RecipesKind::OpenIndianaUserland,
        }
    }
}

impl Into<RecipeKind>  for RecipesKind{
    fn into(self) -> RecipeKind {
        match self {
            RecipesKind::OpenIndianaUserland => RecipeKind::OpenIndianaUserland,
            RecipesKind::Forge => RecipeKind::Forge,
        }
    }
}

#[derive(strum::Display, Enum, Copy, Clone, Eq, PartialEq)]
enum SourceRepoKind {
    Upstream,
    Recipes,
}

impl From<sea_orm_active_enums::SourceRepoKind> for SourceRepoKind {
    fn from(value: sea_orm_active_enums::SourceRepoKind) -> Self {
        match value {
            sea_orm_active_enums::SourceRepoKind::Recipes => SourceRepoKind::Recipes,
            sea_orm_active_enums::SourceRepoKind::Upstream => SourceRepoKind::Upstream,
        }
    }
}

impl Into<sea_orm_active_enums::SourceRepoKind> for SourceRepoKind {
    fn into(self) -> sea_orm_active_enums::SourceRepoKind {
        match self {
            SourceRepoKind::Upstream => sea_orm_active_enums::SourceRepoKind::Upstream,
            SourceRepoKind::Recipes => sea_orm_active_enums::SourceRepoKind::Recipes,
        }
    }
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

    async fn repo_kind(&self) -> SourceRepoKind {
        SourceRepoKind::from(self.0.repo_kind.clone())
    }

    async fn recipe_kind(&self) -> RecipesKind {
        RecipesKind::from(self.0.recipe_kind.clone())
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
            repo_kind: ActiveValue::Set(input.repo_kind.unwrap_or(SourceRepoKind::Upstream).into()),
            recipe_kind: ActiveValue::Set(input.recipes_kind.into()),
        };
        let repo = new_repo.insert(db).await?;
        Ok(SourceRepoObject(repo))
    }
}