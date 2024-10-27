use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

pub struct ReportWorkerWorker {
    pub ctx: AppContext,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct ReportWorkerWorkerArgs {}

#[async_trait]
impl BackgroundWorker<ReportWorkerWorkerArgs> for ReportWorkerWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }
    async fn perform(&self, _args: ReportWorkerWorkerArgs) -> Result<()> {
        println!("=================ReportWorker=======================");
        // TODO: Some actual work goes here...
        Ok(())
    }
}
