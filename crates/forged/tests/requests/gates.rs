use insta::{assert_debug_snapshot, with_settings};
use forged::app::App;
use loco_rs::testing;
use serial_test::serial;
use forged::models::_entities::gates;

// TODO: see how to dedup / extract this to app-local test utils
// not to framework, because that would require a runtime dep on insta
macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_suffix("gate_request");
        let _guard = settings.bind_to_scope();
    };
}

#[tokio::test]
#[serial]
async fn can_add() {
    configure_insta!();

    testing::request::<App, _, _>(|request, ctx| async move {
        let payload = serde_json::json!({
            "id": "a1a6a18e-61bf-4d13-b00b-ff543c91e890",
            "name": "userland",
            "version": "0.5.11",
            "branch": "2024.0.0"
        });

        let _response = request.post("/api/gates").json(&payload).await;
        let saved_gate = gates::Model::find_by_name(&ctx.db, "userland").await;

        with_settings!({
            filters => testing::cleanup_user_model()
        }, {
            assert_debug_snapshot!(saved_gate);
        });
    })
        .await;
}