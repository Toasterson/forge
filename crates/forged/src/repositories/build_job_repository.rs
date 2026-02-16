use crate::entities::build_job;
use miette::{Context, IntoDiagnostic, Result};
use sea_orm::*;

/// Repository for build job CRUD operations.
pub struct BuildJobRepository {
    db: DatabaseConnection,
}

impl BuildJobRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Create a new build job record with status "queued".
    pub async fn create_build_job(
        &self,
        component_id: &str,
        gate_id: &str,
        actor_id: &str,
        request_id: &str,
    ) -> Result<build_job::Model> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().fixed_offset();

        let model = build_job::ActiveModel {
            id: Set(id),
            component_id: Set(component_id.to_string()),
            gate_id: Set(gate_id.to_string()),
            actor_id: Set(actor_id.to_string()),
            request_id: Set(request_id.to_string()),
            status: Set("queued".to_string()),
            exit_code: Set(None),
            summary: Set(None),
            build_log_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            completed_at: Set(None),
        };

        let result = model
            .insert(&self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to insert build_job")?;

        Ok(result)
    }

    /// Get a build job by ID.
    pub async fn get_build_job(&self, id: &str) -> Result<Option<build_job::Model>> {
        build_job::Entity::find_by_id(id.to_string())
            .one(&self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to query build_job")
    }

    /// Find a build job by its solstice-ci request_id.
    pub async fn find_by_request_id(&self, request_id: &str) -> Result<Option<build_job::Model>> {
        build_job::Entity::find()
            .filter(build_job::Column::RequestId.eq(request_id))
            .one(&self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to query build_job by request_id")
    }

    /// List build jobs for a component, ordered by creation time (newest first).
    pub async fn list_by_component(&self, component_id: &str) -> Result<Vec<build_job::Model>> {
        build_job::Entity::find()
            .filter(build_job::Column::ComponentId.eq(component_id))
            .order_by_desc(build_job::Column::CreatedAt)
            .all(&self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to list build_jobs by component")
    }

    /// Update build job status, optionally setting exit_code and summary.
    /// Also sets completed_at for terminal states.
    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
        exit_code: Option<i32>,
        summary: Option<&str>,
    ) -> Result<build_job::Model> {
        let now = chrono::Utc::now().fixed_offset();
        let is_terminal = matches!(status, "success" | "failed" | "cancelled");

        let mut model = build_job::ActiveModel {
            id: Set(id.to_string()),
            status: Set(status.to_string()),
            updated_at: Set(now),
            ..Default::default()
        };

        if let Some(code) = exit_code {
            model.exit_code = Set(Some(code));
        }

        if let Some(s) = summary {
            model.summary = Set(Some(s.to_string()));
        }

        if is_terminal {
            model.completed_at = Set(Some(now));
        }

        let result = model
            .update(&self.db)
            .await
            .into_diagnostic()
            .wrap_err("failed to update build_job status")?;

        Ok(result)
    }
}
