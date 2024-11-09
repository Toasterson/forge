use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

pub struct ArchiveFetcherWorker {
    pub ctx: AppContext,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct ArchiveFetcherWorkerArgs {}

#[async_trait]
impl BackgroundWorker<ArchiveFetcherWorkerArgs> for ArchiveFetcherWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }
    async fn perform(&self, _args: ArchiveFetcherWorkerArgs) -> Result<()> {
        println!("=================ArchiveFetcher=======================");
        // TODO: Some actual work goes here...
        Ok(())
    }
}
