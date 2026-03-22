use crate::entities::audit_log;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use std::sync::Arc;

/// Service for recording audit log entries.
///
/// All logging methods are fire-and-forget: they spawn a background task
/// so the caller is never blocked waiting for the database insert.
#[derive(Clone)]
pub struct AuditService {
    db: Arc<DatabaseConnection>,
}

impl AuditService {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// Record an audit log entry with full control over all fields.
    ///
    /// The insert is spawned onto the Tokio runtime so the caller returns
    /// immediately.
    pub fn log(
        &self,
        actor_id: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        outcome: impl Into<String>,
        details: Option<String>,
    ) {
        let db = self.db.clone();
        let model = audit_log::ActiveModel {
            actor_id: Set(actor_id.into()),
            action: Set(action.into()),
            resource_type: Set(resource_type.into()),
            resource_id: Set(resource_id.into()),
            outcome: Set(outcome.into()),
            details: Set(details),
            ..Default::default()
        };

        tokio::spawn(async move {
            if let Err(e) = model.insert(db.as_ref()).await {
                tracing::warn!(error = %e, "Failed to write audit log entry");
            }
        });
    }

    /// Convenience method to log a successful action.
    pub fn log_success(
        &self,
        actor_id: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) {
        self.log(actor_id, action, resource_type, resource_id, "success", None);
    }

    /// Convenience method to log a denied action.
    pub fn log_denied(
        &self,
        actor_id: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) {
        self.log(actor_id, action, resource_type, resource_id, "denied", None);
    }
}
