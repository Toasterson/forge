use crate::entities::{component, component_file, Component, ComponentFile};
use crate::repositories::{ApplicationBlobType, BlobRepository};
use crate::storage::jj_repos::manager::JjRepoManager;
use crate::types::{ComponentId, RepoId};
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// Repository for component lifecycle management
/// Orchestrates database + SeaweedFS + Jujutsu
#[derive(Clone)]
pub struct ComponentRepository {
    db: Arc<DatabaseConnection>,
    blob_repo: Arc<BlobRepository>,
    jj_manager: Arc<JjRepoManager>,
}

impl ComponentRepository {
    pub fn new(
        db: Arc<DatabaseConnection>,
        blob_repo: Arc<BlobRepository>,
        jj_manager: Arc<JjRepoManager>,
    ) -> Self {
        Self {
            db,
            blob_repo,
            jj_manager,
        }
    }

    /// Create a new component
    /// Creates database record and initializes Jujutsu repository
    pub async fn create_component(
        &self,
        gate_id: &str,
        name: String,
        recipe_kdl: String,
    ) -> Result<component::Model> {
        let component_id = Uuid::new_v4().to_string();

        // 1. Create database record
        let model = component::ActiveModel {
            id: Set(component_id.clone()),
            gate_id: Set(gate_id.to_string()),
            name: Set(name.clone()),
            recipe_kdl: Set(recipe_kdl.clone()),
            ..Default::default()
        };

        let component = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert component")?;

        // 2. Create Jujutsu repository and commit initial files
        let repo_id = RepoId::Component(ComponentId(component_id.clone()));

        let manifest = json!({
            "component_id": component_id,
            "name": name,
            "gate_id": gate_id,
            "recipe_kdl": recipe_kdl,
            "created_at": component.created_at.to_string(),
        });

        self.jj_manager
            .ensure_and_commit(
                &repo_id,
                vec![
                    (
                        "manifest.json".to_string(),
                        serde_json::to_string_pretty(&manifest)
                            .unwrap()
                            .into_bytes(),
                    ),
                    ("recipe.kdl".to_string(), recipe_kdl.into_bytes()),
                ],
                "Initialize component".to_string(),
            )
            .await
            .wrap_err("failed to create Jujutsu repository for component")?;

        tracing::info!(
            component_id = %component_id,
            name = %name,
            gate_id = %gate_id,
            "Created new component"
        );

        Ok(component)
    }

    /// Get a component by ID
    pub async fn get_component(&self, component_id: &str) -> Result<Option<component::Model>> {
        let component = Component::find_by_id(component_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get component")?;

        Ok(component)
    }

    /// Update a component's recipe KDL
    /// Also commits the change to the Jujutsu repository
    pub async fn update_component(
        &self,
        component_id: &str,
        recipe_kdl: String,
    ) -> Result<component::Model> {
        let component = self.get_component(component_id).await?.ok_or_else(|| {
            miette::miette!(
                "Component not found: id={}. \n\
                     This component may have been deleted or does not exist.",
                component_id
            )
        })?;

        // 1. Update database record
        let mut active: component::ActiveModel = component.clone().into();
        active.recipe_kdl = Set(recipe_kdl.clone());
        active.updated_at = Set(chrono::Utc::now().into());

        let updated = active
            .update(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to update component")?;

        // 2. Update Jujutsu repository
        let repo_id = RepoId::Component(ComponentId(component_id.to_string()));

        let manifest = json!({
            "component_id": component_id,
            "name": component.name,
            "gate_id": component.gate_id,
            "recipe_kdl": recipe_kdl,
            "updated_at": updated.updated_at.to_string(),
        });

        self.jj_manager
            .ensure_and_commit(
                &repo_id,
                vec![
                    (
                        "manifest.json".to_string(),
                        serde_json::to_string_pretty(&manifest)
                            .unwrap()
                            .into_bytes(),
                    ),
                    ("recipe.kdl".to_string(), recipe_kdl.into_bytes()),
                ],
                "Update component recipe".to_string(),
            )
            .await
            .wrap_err("failed to update Jujutsu repository for component")?;

        tracing::info!(
            component_id = %component_id,
            "Updated component recipe"
        );

        Ok(updated)
    }

    /// Add a file to a component (patch, license, or script)
    /// Stores the file blob and commits to Jujutsu
    pub async fn add_component_file(
        &self,
        component_id: &str,
        kind: ApplicationBlobType,
        name: String,
        rel_path: String,
        data: &[u8],
    ) -> Result<component_file::Model> {
        // Verify component exists
        self.get_component(component_id).await?.ok_or_else(|| {
            miette::miette!(
                "Component not found: id={}. Cannot add file to non-existent component.",
                component_id
            )
        })?;

        // 1. Store blob via BlobRepository
        let (blob_hash, _fid) = self
            .blob_repo
            .store_blob(data, kind)
            .await
            .wrap_err_with(|| format!("failed to store {} blob", kind))?;

        // 2. Create metadata record
        let model = component_file::ActiveModel {
            component_id: Set(component_id.to_string()),
            kind: Set(kind.to_string()),
            name: Set(name.clone()),
            rel_path: Set(rel_path.clone()),
            blob_hash: Set(blob_hash.clone()),
            size_bytes: Set(data.len() as i64),
            ..Default::default()
        };

        let file = model
            .insert(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert component file record")?;

        // 3. Commit to Jujutsu repository
        let repo_id = RepoId::Component(ComponentId(component_id.to_string()));
        let jj_path = format!("{}/{}", kind, name);

        self.jj_manager
            .ensure_and_commit(
                &repo_id,
                vec![(jj_path, data.to_vec())],
                format!("Add {} file: {}", kind, name),
            )
            .await
            .wrap_err("failed to commit component file to Jujutsu")?;

        tracing::info!(
            component_id = %component_id,
            kind = %kind,
            name = %name,
            blob_hash = %blob_hash,
            "Added component file"
        );

        Ok(file)
    }

    /// List all files of a specific kind for a component
    pub async fn list_component_files(
        &self,
        component_id: &str,
        kind: Option<ApplicationBlobType>,
    ) -> Result<Vec<component_file::Model>> {
        let mut query =
            ComponentFile::find().filter(component_file::Column::ComponentId.eq(component_id));

        if let Some(k) = kind {
            query = query.filter(component_file::Column::Kind.eq(k.to_string()));
        }

        let files = query
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list component files")?;

        Ok(files)
    }

    /// Get a specific component file by ID
    pub async fn get_component_file(&self, file_id: i64) -> Result<Option<component_file::Model>> {
        let file = ComponentFile::find_by_id(file_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to get component file")?;

        Ok(file)
    }

    /// Get component file data
    pub async fn get_component_file_data(&self, file_id: i64) -> Result<Vec<u8>> {
        let file = self.get_component_file(file_id).await?.ok_or_else(|| {
            miette::miette!(
                "Component file not found: id={}. \n\
                     This file may have been deleted or does not exist.",
                file_id
            )
        })?;

        let kind: ApplicationBlobType = file
            .kind
            .parse()
            .map_err(|e: String| miette::miette!("{}", e))?;

        let data = self
            .blob_repo
            .get_blob(&file.blob_hash, kind)
            .await
            .wrap_err_with(|| {
                format!(
                    "failed to get component file data: file_id={}, hash={}",
                    file_id, file.blob_hash
                )
            })?;

        Ok(data)
    }

    /// List all components in a gate
    pub async fn list_by_gate(&self, gate_id: &str) -> Result<Vec<component::Model>> {
        let components = Component::find()
            .filter(component::Column::GateId.eq(gate_id))
            .all(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list components by gate")?;

        Ok(components)
    }

    /// Delete a component
    /// This will cascade delete all component files and source archives
    pub async fn delete(&self, component_id: &str) -> Result<()> {
        Component::delete_by_id(component_id)
            .exec(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to delete component")?;

        tracing::info!(
            component_id = %component_id,
            "Deleted component"
        );

        Ok(())
    }

    /// Check if a component exists
    pub async fn exists(&self, component_id: &str) -> Result<bool> {
        let exists = Component::find_by_id(component_id)
            .one(&*self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to check component existence")?
            .is_some();

        Ok(exists)
    }

    /// Get all files for a component organized by kind
    pub async fn get_all_component_files(&self, component_id: &str) -> Result<ComponentFiles> {
        let all_files = self.list_component_files(component_id, None).await?;

        let mut patches = Vec::new();
        let mut licenses = Vec::new();
        let mut scripts = Vec::new();

        for file in all_files {
            match file.kind.as_str() {
                "patch" => patches.push(file),
                "license" => licenses.push(file),
                "script" => scripts.push(file),
                _ => {
                    tracing::warn!(
                        component_id = %component_id,
                        kind = %file.kind,
                        "Unknown component file kind"
                    );
                }
            }
        }

        Ok(ComponentFiles {
            patches,
            licenses,
            scripts,
        })
    }
}

/// Organized component files by kind
#[derive(Debug, Clone)]
pub struct ComponentFiles {
    pub patches: Vec<component_file::Model>,
    pub licenses: Vec<component_file::Model>,
    pub scripts: Vec<component_file::Model>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would go here
    // They require a database connection, BlobRepository, and Jujutsu manager
}
