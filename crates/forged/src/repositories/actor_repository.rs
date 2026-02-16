use crate::entities::{actor, actor_key, Actor, ActorKey};
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
        let actor = self.get_by_id(actor_id).await?.ok_or_else(|| {
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

    // ===== Registration =====

    /// Create a new unconfirmed actor with an SSH key for registration
    pub async fn create_unconfirmed(
        &self,
        display_name: String,
        email: String,
        challenge: String,
    ) -> Result<actor::Model> {
        let actor_id = Uuid::new_v4().to_string();
        let model = actor::ActiveModel {
            id: Set(actor_id.clone()),
            kind: Set("user".to_string()),
            oidc_sub: Set(None),
            display_name: Set(display_name.clone()),
            email: Set(Some(email)),
            confirmed: Set(false),
            confirmation_challenge: Set(Some(challenge)),
            ..Default::default()
        };

        let actor = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert unconfirmed actor")?;

        tracing::info!(
            actor_id = %actor_id,
            display_name = %display_name,
            "Created unconfirmed actor for registration"
        );

        Ok(actor)
    }

    /// Confirm an actor's registration
    pub async fn confirm_actor(&self, actor_id: &str) -> Result<actor::Model> {
        let actor = self.get_by_id(actor_id).await?.ok_or_else(|| {
            miette::miette!(
                "Actor not found: id={}.\n\
                 The registration may have expired or the actor ID is incorrect.",
                actor_id
            )
        })?;

        let mut active: actor::ActiveModel = actor.into();
        active.confirmed = Set(true);
        active.confirmation_challenge = Set(None);
        active.updated_at = Set(chrono::Utc::now().into());

        let updated = active
            .update(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to confirm actor")?;

        tracing::info!(actor_id = %actor_id, "Actor registration confirmed");
        Ok(updated)
    }

    // ===== Actor Key CRUD =====

    /// Store an SSH public key for an actor
    pub async fn add_key(
        &self,
        actor_id: &str,
        key_id: String,
        algorithm: String,
        public_key: String,
    ) -> Result<actor_key::Model> {
        let model = actor_key::ActiveModel {
            actor_id: Set(actor_id.to_string()),
            key_id: Set(key_id.clone()),
            algorithm: Set(algorithm),
            public_key: Set(public_key),
            ..Default::default()
        };

        let key = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to store SSH key '{}' for actor {}.\n\
                     A key with this label may already exist. Choose a different key_id.",
                    key_id, actor_id
                )
            })?;

        tracing::info!(
            actor_id = %actor_id,
            key_id = %key_id,
            "Stored new SSH key for actor"
        );

        Ok(key)
    }

    /// List all SSH keys for an actor
    pub async fn list_keys(&self, actor_id: &str) -> Result<Vec<actor_key::Model>> {
        let keys = ActorKey::find()
            .filter(actor_key::Column::ActorId.eq(actor_id))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list actor keys")?;

        Ok(keys)
    }

    /// Get a specific key by actor_id and key_id
    pub async fn get_key(&self, actor_id: &str, key_id: &str) -> Result<Option<actor_key::Model>> {
        let key = ActorKey::find()
            .filter(actor_key::Column::ActorId.eq(actor_id))
            .filter(actor_key::Column::KeyId.eq(key_id))
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get actor key")?;

        Ok(key)
    }

    /// Delete an SSH key
    pub async fn delete_key(&self, actor_id: &str, key_id: &str) -> Result<()> {
        let key = self.get_key(actor_id, key_id).await?.ok_or_else(|| {
            miette::miette!(
                "SSH key not found: actor={}, key_id={}.\n\
                 The key may have been already removed.",
                actor_id,
                key_id
            )
        })?;

        let active: actor_key::ActiveModel = key.into();
        active
            .delete(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to delete actor key")?;

        tracing::info!(
            actor_id = %actor_id,
            key_id = %key_id,
            "Deleted SSH key"
        );

        Ok(())
    }
}
