use crate::entities::{actor, Actor};
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::sync::Arc;
use uuid::Uuid;

/// Repository for actor (user/service) management
#[derive(Clone)]
pub struct ActorRepository {
    db: Arc<DatabaseConnection>,
}

impl ActorRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// Create or update an actor from OIDC claims
    /// If an actor with the same oidc_sub exists, returns the existing actor
    /// Otherwise creates a new actor
    pub async fn create_or_update_from_oidc(
        &self,
        oidc_sub: String,
        display_name: String,
    ) -> Result<actor::Model> {
        // Check if actor already exists
        if let Some(existing) = self.get_by_oidc_sub(&oidc_sub).await? {
            // Update display name if changed
            if existing.display_name != display_name {
                let mut active: actor::ActiveModel = existing.into();
                active.display_name = Set(display_name);
                active.updated_at = Set(chrono::Utc::now().into());

                let updated = active
                    .update(&*self.db)
                    .await
                    .into_diagnostic()
                    .wrap_err("failed to update actor")?;

                tracing::info!(
                    actor_id = %updated.id,
                    display_name = %updated.display_name,
                    "Updated actor from OIDC"
                );

                return Ok(updated);
            }
            return Ok(existing);
        }

        // Create new actor
        let actor_id = Uuid::new_v4().to_string();
        let model = actor::ActiveModel {
            id: Set(actor_id.clone()),
            kind: Set("user".to_string()),
            oidc_sub: Set(Some(oidc_sub)),
            display_name: Set(display_name.clone()),
            ..Default::default()
        };

        let actor = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert actor")?;

        tracing::info!(
            actor_id = %actor_id,
            display_name = %display_name,
            "Created new actor from OIDC"
        );

        Ok(actor)
    }

    /// Create a service actor (no OIDC sub)
    pub async fn create_service(&self, display_name: String) -> Result<actor::Model> {
        let actor_id = Uuid::new_v4().to_string();
        let model = actor::ActiveModel {
            id: Set(actor_id.clone()),
            kind: Set("service".to_string()),
            oidc_sub: Set(None),
            display_name: Set(display_name.clone()),
            ..Default::default()
        };

        let actor = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert service actor")?;

        tracing::info!(
            actor_id = %actor_id,
            display_name = %display_name,
            "Created new service actor"
        );

        Ok(actor)
    }

    /// Get an actor by ID
    pub async fn get_by_id(&self, actor_id: &str) -> Result<Option<actor::Model>> {
        let actor = Actor::find_by_id(actor_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get actor by ID")?;

        Ok(actor)
    }

    /// Get an actor by OIDC subject claim
    pub async fn get_by_oidc_sub(&self, oidc_sub: &str) -> Result<Option<actor::Model>> {
        let actor = Actor::find()
            .filter(actor::Column::OidcSub.eq(oidc_sub))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get actor by OIDC sub")?;

        Ok(actor)
    }

    /// List all actors
    pub async fn list_all(&self) -> Result<Vec<actor::Model>> {
        let actors = Actor::find()
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list actors")?;

        Ok(actors)
    }

    /// List actors by kind (user or service)
    pub async fn list_by_kind(&self, kind: &str) -> Result<Vec<actor::Model>> {
        let actors = Actor::find()
            .filter(actor::Column::Kind.eq(kind))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list actors by kind")?;

        Ok(actors)
    }

    /// Update actor display name
    pub async fn update_display_name(
        &self,
        actor_id: &str,
        display_name: String,
    ) -> Result<actor::Model> {
        let actor = self
            .get_by_id(actor_id)
            .await?
            .ok_or_else(|| {
                miette::miette!(
                    "Actor not found: id={}. \n\
                     This actor may have been deleted or does not exist.",
                    actor_id
                )
            })?;

        let mut active: actor::ActiveModel = actor.into();
        active.display_name = Set(display_name);
        active.updated_at = Set(chrono::Utc::now().into());

        let updated = active
            .update(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to update actor display name")?;

        tracing::info!(
            actor_id = %actor_id,
            new_display_name = %updated.display_name,
            "Updated actor display name"
        );

        Ok(updated)
    }

    /// Delete an actor
    /// Note: This will fail if the actor owns gates or is a gate member due to foreign key constraints
    pub async fn delete(&self, actor_id: &str) -> Result<()> {
        Actor::delete_by_id(actor_id)
            .exec(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to delete actor: id={}. \n\
                     This actor may own gates or be a member of gates. \n\
                     Remove the actor from all gates first.",
                    actor_id
                )
            })?;

        tracing::info!(
            actor_id = %actor_id,
            "Deleted actor"
        );

        Ok(())
    }

    /// Check if an actor exists
    pub async fn exists(&self, actor_id: &str) -> Result<bool> {
        let exists = Actor::find_by_id(actor_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to check actor existence")?
            .is_some();

        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would go here
    // They require a database connection
}
