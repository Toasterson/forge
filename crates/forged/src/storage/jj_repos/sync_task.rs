use super::{JjRepoManager, OpLogSync};
use std::sync::Arc;
use std::time::Duration;

/// Background task for periodic operation log synchronization
///
/// This task runs in a loop and:
/// 1. Publishes local operations to PostgreSQL
/// 2. Fetches operations from other replicas
/// 3. Triggers automatic merging in Jujutsu
pub async fn run_sync_task(
    sync: Arc<OpLogSync>,
    repo_manager: Arc<JjRepoManager>,
    interval: Duration,
) {
    let mut interval_timer = tokio::time::interval(interval);

    loop {
        interval_timer.tick().await;

        tracing::debug!("sync task: starting sync cycle");

        // Get all active repos
        let repos = repo_manager.list_all_repos().await;

        for repo_id in repos {
            // Publish local ops
            if let Err(e) = sync.publish_operations(&repo_id).await {
                tracing::error!(
                    repo_id = %repo_id,
                    error = ?e,
                    "failed to publish operations"
                );
            }

            // Fetch and merge remote ops
            if let Err(e) = sync.fetch_and_merge(&repo_id).await {
                tracing::error!(
                    repo_id = %repo_id,
                    error = ?e,
                    "failed to fetch and merge operations"
                );
            }
        }

        tracing::debug!("sync task: completed sync cycle");
    }
}
