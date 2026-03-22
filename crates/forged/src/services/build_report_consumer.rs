//! Background consumer for build job results from RabbitMQ.
//!
//! Listens on the results queue and updates build_job records when
//! solstice-ci workers report completion.

use crate::repositories::BuildJobRepository;
use crate::services::build_dispatch::{AmqpConfig, JobResult};
use miette::{IntoDiagnostic, Result};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Background task that consumes JobResult messages and updates build_job records.
pub struct BuildReportConsumer {
    build_job_repo: Arc<BuildJobRepository>,
    amqp_pool: deadpool_lapin::Pool,
    amqp_config: AmqpConfig,
    cancel: CancellationToken,
}

impl BuildReportConsumer {
    pub fn new(
        build_job_repo: Arc<BuildJobRepository>,
        amqp_pool: deadpool_lapin::Pool,
        amqp_config: AmqpConfig,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            build_job_repo,
            amqp_pool,
            amqp_config,
            cancel,
        }
    }

    /// Run the consumer loop. Returns when cancelled or on unrecoverable error.
    ///
    /// Uses exponential backoff (1s -> 2s -> 4s -> ... -> 60s max) between
    /// reconnection attempts instead of a fixed interval.
    pub async fn run(self) -> Result<()> {
        const INITIAL_BACKOFF_SECS: u64 = 1;
        const MAX_BACKOFF_SECS: u64 = 60;
        const BACKOFF_MULTIPLIER: u64 = 2;

        let mut backoff_secs = INITIAL_BACKOFF_SECS;

        loop {
            if self.cancel.is_cancelled() {
                tracing::info!("Build report consumer shutting down");
                return Ok(());
            }

            tracing::info!("Connecting build report consumer...");
            match self.consume_loop().await {
                Ok(()) => {
                    tracing::info!("Build report consumer loop ended normally");
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        backoff_secs = backoff_secs,
                        "Build report consumer error, retrying in {}s...",
                        backoff_secs
                    );

                    tokio::select! {
                        _ = self.cancel.cancelled() => {
                            tracing::info!("Build report consumer shutting down during backoff");
                            return Ok(());
                        }
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)) => {}
                    }

                    // Exponential backoff with cap.
                    backoff_secs = (backoff_secs * BACKOFF_MULTIPLIER).min(MAX_BACKOFF_SECS);
                }
            }
        }
    }

    async fn consume_loop(&self) -> Result<()> {
        use deadpool_lapin::lapin::options::*;
        use deadpool_lapin::lapin::types::FieldTable;
        use futures_util::StreamExt;

        let conn = self.amqp_pool.get().await.into_diagnostic()?;
        let channel = conn.create_channel().await.into_diagnostic()?;

        let mut consumer = channel
            .basic_consume(
                &self.amqp_config.results_queue,
                "forged.build-report-consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .into_diagnostic()?;

        tracing::info!(
            queue = %self.amqp_config.results_queue,
            "Build report consumer connected, waiting for results"
        );

        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    tracing::info!("Build report consumer received shutdown signal");
                    return Ok(());
                }
                delivery = consumer.next() => {
                    match delivery {
                        Some(Ok(delivery)) => {
                            let tag = delivery.delivery_tag;
                            match self.handle_result(&delivery.data).await {
                                Ok(()) => {
                                    let _ = channel
                                        .basic_ack(tag, BasicAckOptions::default())
                                        .await;
                                }
                                Err(e) => {
                                    tracing::error!(error = %e, "Failed to process build result");
                                    let _ = channel
                                        .basic_nack(tag, BasicNackOptions::default())
                                        .await;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            return Err(miette::miette!("Consumer error: {}", e));
                        }
                        None => {
                            tracing::warn!("Consumer stream ended");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    async fn handle_result(&self, data: &[u8]) -> Result<()> {
        let result: JobResult = serde_json::from_slice(data)
            .into_diagnostic()
            .map_err(|e| {
                miette::miette!(
                    "Failed to deserialize job result: {}\n\
                     Raw payload: {}",
                    e,
                    String::from_utf8_lossy(data)
                )
            })?;

        let request_id = result.request_id.to_string();
        let status = if result.success { "success" } else { "failed" };

        tracing::info!(
            request_id = %request_id,
            success = result.success,
            exit_code = result.exit_code,
            "Received build result"
        );

        // Find the build job by request_id and update it
        match self.build_job_repo.find_by_request_id(&request_id).await? {
            Some(job) => {
                self.build_job_repo
                    .update_status(
                        &job.id,
                        status,
                        Some(result.exit_code),
                        result.summary.as_deref(),
                    )
                    .await?;

                tracing::info!(
                    build_job_id = %job.id,
                    request_id = %request_id,
                    status = %status,
                    "Build job status updated"
                );
            }
            None => {
                tracing::warn!(
                    request_id = %request_id,
                    "Received result for unknown build job (request_id not found)"
                );
            }
        }

        Ok(())
    }
}
