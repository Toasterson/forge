mod common;
use common::{fixtures::TestFixtures, TestContext};

#[tokio::test]
async fn test_owner_has_all_permissions() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Owner should have all permissions
    assert!(
        ctx.app_state
            .rbac
            .check_gate_read(&owner.id, &gate.id)
            .await
            .unwrap()
    );
    assert!(
        ctx.app_state
            .rbac
            .check_gate_write(&owner.id, &gate.id)
            .await
            .unwrap()
    );
    assert!(
        ctx.app_state
            .rbac
            .check_gate_admin(&owner.id, &gate.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_non_member_denied() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let stranger = TestFixtures::actor(&ctx, "stranger").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Stranger should have no permissions
    assert!(
        !ctx.app_state
            .rbac
            .check_gate_read(&stranger.id, &gate.id)
            .await
            .unwrap()
    );
    assert!(
        !ctx.app_state
            .rbac
            .check_gate_write(&stranger.id, &gate.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_permission_inheritance() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let member = TestFixtures::actor(&ctx, "member").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Add member with read permission
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &member.id,
            vec!["reader".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
            ],
        )
        .await
        .expect("Failed to add member");

    // Member should have component read (inherited from gate)
    assert!(
        ctx.app_state
            .rbac
            .check_component_read(&member.id, &component.id)
            .await
            .unwrap()
    );
    assert!(
        !ctx.app_state
            .rbac
            .check_component_write(&member.id, &component.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_member_with_write_permission() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let developer = TestFixtures::actor(&ctx, "developer").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Add developer with write permission
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &developer.id,
            vec!["developer".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
                "component_write".to_string(),
            ],
        )
        .await
        .expect("Failed to add member");

    // Developer should have component write
    assert!(
        ctx.app_state
            .rbac
            .check_component_read(&developer.id, &component.id)
            .await
            .unwrap()
    );
    assert!(
        ctx.app_state
            .rbac
            .check_component_write(&developer.id, &component.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_admin_permission() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let admin = TestFixtures::actor(&ctx, "admin").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Add admin with gate_admin permission
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &admin.id,
            vec!["admin".to_string()],
            vec![
                "gate_read".to_string(),
                "gate_write".to_string(),
                "gate_admin".to_string(),
                "component_read".to_string(),
                "component_write".to_string(),
            ],
        )
        .await
        .expect("Failed to add admin");

    // Admin should have all permissions
    assert!(
        ctx.app_state
            .rbac
            .check_gate_read(&admin.id, &gate.id)
            .await
            .unwrap()
    );
    assert!(
        ctx.app_state
            .rbac
            .check_gate_write(&admin.id, &gate.id)
            .await
            .unwrap()
    );
    assert!(
        ctx.app_state
            .rbac
            .check_gate_admin(&admin.id, &gate.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_multiple_components_in_gate() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let member = TestFixtures::actor(&ctx, "member").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    let component1 = TestFixtures::component(&ctx, &owner, &gate, "component-1").await;
    let component2 = TestFixtures::component(&ctx, &owner, &gate, "component-2").await;

    // Add member with read permission
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &member.id,
            vec!["reader".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
            ],
        )
        .await
        .expect("Failed to add member");

    // Member should have read access to both components
    assert!(
        ctx.app_state
            .rbac
            .check_component_read(&member.id, &component1.id)
            .await
            .unwrap()
    );
    assert!(
        ctx.app_state
            .rbac
            .check_component_read(&member.id, &component2.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_permission_isolation_between_gates() {
    let ctx = TestContext::new().await;
    let owner1 = TestFixtures::actor(&ctx, "owner1").await;
    let owner2 = TestFixtures::actor(&ctx, "owner2").await;
    let member = TestFixtures::actor(&ctx, "member").await;

    let gate1 = TestFixtures::gate(&ctx, &owner1, "gate-1").await;
    let gate2 = TestFixtures::gate(&ctx, &owner2, "gate-2").await;

    // Add member to gate1 only
    ctx.app_state
        .gate_manager
        .add_member(
            &owner1.id,
            &gate1.id,
            &member.id,
            vec!["reader".to_string()],
            vec!["gate_read".to_string()],
        )
        .await
        .expect("Failed to add member");

    // Member should have access to gate1 but not gate2
    assert!(
        ctx.app_state
            .rbac
            .check_gate_read(&member.id, &gate1.id)
            .await
            .unwrap()
    );
    assert!(
        !ctx.app_state
            .rbac
            .check_gate_read(&member.id, &gate2.id)
            .await
            .unwrap()
    );
}
