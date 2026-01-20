mod common;
use common::{fixtures::TestFixtures, TestContext};

#[tokio::test]
async fn test_gate_creation_with_jujutsu() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;

    let gate = ctx
        .app_state
        .gate_manager
        .create_gate(&owner.id, "test-gate".to_string(), "gate { }".to_string())
        .await
        .expect("Failed to create gate");

    assert_eq!(gate.name, "test-gate");
    assert_eq!(gate.owner_id, owner.id);

    // Verify Jujutsu repo exists
    let repo_path = format!(
        "{}/gates/{}",
        ctx.app_state.settings.jj_repos.root, gate.id
    );
    assert!(std::path::Path::new(&repo_path).exists());
}

#[tokio::test]
async fn test_gate_member_management() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let member = TestFixtures::actor(&ctx, "member").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Add member
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &member.id,
            vec!["developer".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
            ],
        )
        .await
        .expect("Failed to add member");

    // Verify member permissions
    let has_read = ctx
        .app_state
        .rbac
        .check_gate_read(&member.id, &gate.id)
        .await
        .expect("Failed to check permission");

    assert!(has_read);
}

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
async fn test_update_gate() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Update gate KDL
    let updated = ctx
        .app_state
        .gate_manager
        .update_gate(
            &owner.id,
            &gate.id,
            "gate { name = \"updated\" }".to_string(),
        )
        .await
        .expect("Failed to update gate");

    assert_eq!(updated.gate_kdl, "gate { name = \"updated\" }");
}

#[tokio::test]
async fn test_unauthorized_gate_update() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let stranger = TestFixtures::actor(&ctx, "stranger").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Stranger cannot update gate
    let result = ctx
        .app_state
        .gate_manager
        .update_gate(
            &stranger.id,
            &gate.id,
            "gate { name = \"hacked\" }".to_string(),
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_gate_members() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let member1 = TestFixtures::actor(&ctx, "member1").await;
    let member2 = TestFixtures::actor(&ctx, "member2").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Add members
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &member1.id,
            vec!["developer".to_string()],
            vec!["gate_read".to_string()],
        )
        .await
        .expect("Failed to add member1");

    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &member2.id,
            vec!["reviewer".to_string()],
            vec!["gate_read".to_string()],
        )
        .await
        .expect("Failed to add member2");

    // List members
    let members = ctx
        .app_state
        .gate_manager
        .list_members(&owner.id, &gate.id)
        .await
        .expect("Failed to list members");

    assert_eq!(members.len(), 2);
}
