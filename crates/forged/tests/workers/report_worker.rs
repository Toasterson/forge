use forged::app::App;
use loco_rs::prelude::*;
use loco_rs::testing;

use forged::workers::report_worker::ReportWorkerWorker;
use forged::workers::report_worker::ReportWorkerWorkerArgs;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_run_report_worker_worker() {
    let boot = testing::boot_test::<App>().await.unwrap();

    // Execute the worker ensuring that it operates in 'ForegroundBlocking' mode, which prevents the addition of your worker to the background
    assert!(
        ReportWorkerWorker::perform_later(&boot.app_context, ReportWorkerWorkerArgs {})
            .await
            .is_ok()
    );
    // Include additional assert validations after the execution of the worker
}
