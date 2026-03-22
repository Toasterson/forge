// End-to-end tests for the Forge V2 gRPC API.
//
// These tests require running services (PostgreSQL, SeaweedFS, RabbitMQ).
// Run with: cargo test --test e2e_git -- --ignored --nocapture
//
// TODO: Rewrite e2e tests for the V2 architecture.
// The previous tests targeted the V1 API (SurrealDB, git storage, old protobuf).

#[tokio::test]
#[ignore]
async fn e2e_placeholder() {
    // This test is a placeholder. The V1 e2e tests have been removed because they
    // referenced modules that no longer exist (SurrealDB, git storage, V1 protobuf).
    //
    // V2 e2e tests should:
    // 1. Start forged with a test AppState
    // 2. Register an actor via AuthService
    // 3. Create a gate via GateService
    // 4. Create a component and upload files via ComponentService
    // 5. Submit a build via BuildService
    // 6. Verify health check responds SERVING
    eprintln!("E2E tests need to be written for V2 architecture");
}
