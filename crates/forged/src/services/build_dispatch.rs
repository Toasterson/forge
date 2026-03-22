//! Build dispatch service: publishes build jobs to RabbitMQ and tracks status.
//!
//! Message types are wire-compatible with solstice-ci `common` crate's
//! `JobRequest` / `JobResult` JSON schema.

use crate::entities::build_job;
use crate::repositories::BuildJobRepository;
use crate::services::RbacService;
use miette::{Context, IntoDiagnostic, Result};
use std::sync::Arc;

/// Solstice-CI compatible job request (wire format: `jobrequest.v1`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobRequest {
    pub schema_version: String,
    pub request_id: uuid::Uuid,
    #[serde(default)]
    pub group_id: Option<uuid::Uuid>,
    pub source: String, // "Manual", "Github", "Forgejo"
    pub repo_url: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub commit_sha: String,
    #[serde(default)]
    pub workflow_path: Option<String>,
    #[serde(default)]
    pub workflow_job_id: Option<String>,
    #[serde(default)]
    pub runs_on: Option<String>,
    #[serde(default)]
    pub script_path: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub submitted_at: time::OffsetDateTime,
}

impl JobRequest {
    pub fn new_manual(
        repo_url: String,
        repo_owner: String,
        repo_name: String,
        commit_sha: String,
    ) -> Self {
        Self {
            schema_version: "jobrequest.v1".to_string(),
            request_id: uuid::Uuid::new_v4(),
            group_id: None,
            source: "Manual".to_string(),
            repo_url,
            repo_owner,
            repo_name,
            commit_sha,
            workflow_path: None,
            workflow_job_id: None,
            runs_on: None,
            script_path: None,
            submitted_at: time::OffsetDateTime::now_utc(),
        }
    }
}

/// Solstice-CI compatible job result (wire format: `jobresult.v1`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobResult {
    pub schema_version: String,
    pub request_id: uuid::Uuid,
    pub repo_url: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub commit_sha: String,
    pub success: bool,
    pub exit_code: i32,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub completed_at: time::OffsetDateTime,
}

/// AMQP configuration for build dispatch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AmqpConfig {
    #[serde(default = "default_amqp_url")]
    pub url: String,
    #[serde(default = "default_exchange")]
    pub exchange: String,
    #[serde(default = "default_routing_key")]
    pub routing_key: String,
    #[serde(default = "default_queue")]
    pub queue: String,
    #[serde(default = "default_results_routing_key")]
    pub results_routing_key: String,
    #[serde(default = "default_results_queue")]
    pub results_queue: String,
}

impl Default for AmqpConfig {
    fn default() -> Self {
        Self {
            url: default_amqp_url(),
            exchange: default_exchange(),
            routing_key: default_routing_key(),
            queue: default_queue(),
            results_routing_key: default_results_routing_key(),
            results_queue: default_results_queue(),
        }
    }
}

fn default_amqp_url() -> String {
    "amqp://dev:dev@localhost:5672/master".to_string()
}
fn default_exchange() -> String {
    "build_jobs".to_string()
}
fn default_routing_key() -> String {
    "build.submit".to_string()
}
fn default_queue() -> String {
    "build_jobs".to_string()
}
fn default_results_routing_key() -> String {
    "build.result".to_string()
}
fn default_results_queue() -> String {
    "build_results".to_string()
}

/// Build dispatch service: submit builds, query status, cancel jobs.
#[derive(Clone)]
pub struct BuildDispatchService {
    build_job_repo: Arc<BuildJobRepository>,
    rbac: Arc<RbacService>,
    amqp_pool: deadpool_lapin::Pool,
    amqp_config: AmqpConfig,
}

impl BuildDispatchService {
    pub fn new(
        build_job_repo: Arc<BuildJobRepository>,
        rbac: Arc<RbacService>,
        amqp_pool: deadpool_lapin::Pool,
        amqp_config: AmqpConfig,
    ) -> Self {
        Self {
            build_job_repo,
            rbac,
            amqp_pool,
            amqp_config,
        }
    }

    /// Declare AMQP topology (exchanges + queues + bindings).
    pub async fn declare_topology(&self) -> Result<()> {
        use deadpool_lapin::lapin::options::*;
        use deadpool_lapin::lapin::types::FieldTable;
        use deadpool_lapin::lapin::ExchangeKind;

        let conn = self.amqp_pool.get().await.into_diagnostic().wrap_err(
            "Failed to get AMQP connection from pool.\n\
             Ensure RabbitMQ is running and the AMQP URL is correct.",
        )?;
        let channel = conn.create_channel().await.into_diagnostic()?;

        // Job submission exchange + queue
        channel
            .exchange_declare(
                &self.amqp_config.exchange,
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .into_diagnostic()?;

        channel
            .queue_declare(
                &self.amqp_config.queue,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .into_diagnostic()?;

        channel
            .queue_bind(
                &self.amqp_config.queue,
                &self.amqp_config.exchange,
                &self.amqp_config.routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .into_diagnostic()?;

        // Results exchange + queue
        channel
            .exchange_declare(
                &self.amqp_config.exchange,
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .into_diagnostic()?;

        channel
            .queue_declare(
                &self.amqp_config.results_queue,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .into_diagnostic()?;

        channel
            .queue_bind(
                &self.amqp_config.results_queue,
                &self.amqp_config.exchange,
                &self.amqp_config.results_routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .into_diagnostic()?;

        tracing::info!("AMQP topology declared successfully");
        Ok(())
    }

    /// Submit a build for a component.
    pub async fn submit_build(
        &self,
        actor_id: &str,
        component_id: &str,
        gate_id: &str,
    ) -> Result<build_job::Model> {
        // Check permission
        if !self
            .rbac
            .check_component_write(actor_id, component_id)
            .await?
        {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have write access to component {}.\n\
                 Only component maintainers can submit builds.",
                actor_id,
                component_id
            ));
        }

        // Build a JobRequest
        let job_request = JobRequest::new_manual(
            String::new(), // repo_url — populated from component metadata if available
            String::new(), // repo_owner
            String::new(), // repo_name
            String::new(), // commit_sha
        );

        // Store build job record
        let build_job = self
            .build_job_repo
            .create_build_job(
                component_id,
                gate_id,
                actor_id,
                &job_request.request_id.to_string(),
            )
            .await
            .wrap_err("failed to create build job record")?;

        // Publish to AMQP with retry (3 attempts, 1s backoff).
        // The request_id serves as an idempotency key — it is unique per job,
        // so duplicate publishes are safe.
        let mut last_err = None;
        for attempt in 1..=3u32 {
            match self.publish_job(&job_request).await {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(e) => {
                    tracing::warn!(
                        attempt,
                        request_id = %job_request.request_id,
                        error = ?e,
                        "AMQP publish attempt failed, retrying"
                    );
                    last_err = Some(e);
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }

        if let Some(publish_err) = last_err {
            // All retries exhausted — mark the job as failed so it is not orphaned.
            tracing::error!(
                build_job_id = %build_job.id,
                request_id = %job_request.request_id,
                "All AMQP publish retries exhausted, marking job as failed"
            );

            let summary = format!(
                "Build dispatch failed after 3 attempts: {:#}. \
                 Re-submit the build once RabbitMQ connectivity is restored.",
                publish_err
            );

            let failed_job = self
                .build_job_repo
                .update_status(&build_job.id, "failed", None, Some(&summary))
                .await
                .wrap_err("failed to mark orphaned build job as failed")?;

            return Ok(failed_job);
        }

        tracing::info!(
            build_job_id = %build_job.id,
            request_id = %job_request.request_id,
            component_id = %component_id,
            submitted_by = %actor_id,
            "Build job submitted"
        );

        Ok(build_job)
    }

    /// Publish a job request to the AMQP exchange.
    async fn publish_job(&self, job: &JobRequest) -> Result<()> {
        use deadpool_lapin::lapin::options::BasicPublishOptions;
        use deadpool_lapin::lapin::BasicProperties;

        let conn = self.amqp_pool.get().await.into_diagnostic()?;
        let channel = conn.create_channel().await.into_diagnostic()?;

        let payload = serde_json::to_vec(job).into_diagnostic()?;

        channel
            .basic_publish(
                &self.amqp_config.exchange,
                &self.amqp_config.routing_key,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default()
                    .with_delivery_mode(2) // persistent
                    .with_content_type("application/json".into()),
            )
            .await
            .into_diagnostic()?
            .await
            .into_diagnostic()?;

        Ok(())
    }

    /// Get build job status.
    pub async fn get_build_status(&self, actor_id: &str, job_id: &str) -> Result<build_job::Model> {
        let job = self
            .build_job_repo
            .get_build_job(job_id)
            .await?
            .ok_or_else(|| miette::miette!("Build job not found: {}", job_id))?;

        // Check permission — actor needs read access to the component
        if !self
            .rbac
            .check_component_read(actor_id, &job.component_id)
            .await?
        {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have read access to component {}",
                actor_id,
                job.component_id
            ));
        }

        Ok(job)
    }

    /// List builds for a component.
    pub async fn list_builds(
        &self,
        actor_id: &str,
        component_id: &str,
    ) -> Result<Vec<build_job::Model>> {
        if !self
            .rbac
            .check_component_read(actor_id, component_id)
            .await?
        {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have read access to component {}",
                actor_id,
                component_id
            ));
        }

        let builds = self
            .build_job_repo
            .list_by_component(component_id)
            .await
            .wrap_err("failed to list builds")?;

        Ok(builds)
    }

    /// Cancel a build job.
    pub async fn cancel_build(&self, actor_id: &str, job_id: &str) -> Result<build_job::Model> {
        let job = self
            .build_job_repo
            .get_build_job(job_id)
            .await?
            .ok_or_else(|| miette::miette!("Build job not found: {}", job_id))?;

        // Check permission — need write access to the component
        if !self
            .rbac
            .check_component_write(actor_id, &job.component_id)
            .await?
        {
            return Err(miette::miette!(
                "Permission denied: actor {} does not have write access to component {}",
                actor_id,
                job.component_id
            ));
        }

        if job.status == "success" || job.status == "failed" {
            return Err(miette::miette!(
                "Build job {} has already completed with status '{}'.\n\
                 Only queued or running builds can be cancelled.",
                job_id,
                job.status
            ));
        }

        let updated = self
            .build_job_repo
            .update_status(job_id, "cancelled", None, None)
            .await
            .wrap_err("failed to cancel build job")?;

        tracing::info!(
            build_job_id = %job_id,
            cancelled_by = %actor_id,
            "Build job cancelled"
        );

        Ok(updated)
    }
}
