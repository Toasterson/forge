// Integration tests for Forged V2
// These tests require a running PostgreSQL and SeaweedFS instance
//
// To run tests:
// 1. Start docker-compose services: docker-compose up -d postgres seaweedfs-master seaweedfs-volume
// 2. Run tests: cargo test --test integration_test -- --test-threads=1
//
// NOTE: Tests run sequentially to avoid database conflicts

use forged::entities::server_member;
use forged::{AppState, Settings};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

/// Test database connection and migrations
#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn test_database_connection() {
    let settings = Settings {
        postgres: forged::settings::PostgresConfig {
            url: std::env::var("TEST_DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://forged:forged@localhost/forged_test".to_string()),
            max_connections: 5,
            ..Default::default()
        },
        seaweedfs: forged::settings::SeaweedFsConfig {
            master_url: std::env::var("TEST_SEAWEEDFS_URL")
                .unwrap_or_else(|_| "http://localhost:9333".to_string()),
            namespace: "test".to_string(),
            ..Default::default()
        },
        jj_repos: forged::settings::JjReposConfig {
            root: "./test_data/jj-repos".to_string(),
        },
        ..Default::default()
    };

    // This will run migrations
    let result = AppState::new(settings).await;
    assert!(
        result.is_ok(),
        "Failed to initialize AppState: {:?}",
        result.err()
    );

    let app_state = result.unwrap();
    app_state.shutdown().await.expect("Failed to shutdown");
}

/// Test actor creation from OIDC
#[tokio::test]
#[ignore]
async fn test_actor_creation() {
    let app_state = setup_test_app_state().await;

    // Create actor from OIDC claims
    let actor = app_state
        .actor_repo
        .create_or_update_from_oidc("test_sub_123".to_string(), "Test User".to_string())
        .await
        .expect("Failed to create actor");

    assert_eq!(actor.kind, "user");
    assert_eq!(actor.display_name, "Test User");
    assert_eq!(actor.oidc_sub, Some("test_sub_123".to_string()));

    // Cleanup
    app_state.shutdown().await.expect("Failed to shutdown");
    cleanup_test_data();
}

/// Test gate creation and Jujutsu integration
#[tokio::test]
#[ignore]
async fn test_gate_lifecycle() {
    let app_state = setup_test_app_state().await;

    // Create actor
    let actor = app_state
        .actor_repo
        .create_or_update_from_oidc("test_owner".to_string(), "Owner".to_string())
        .await
        .expect("Failed to create actor");

    // Grant gate_create permission and create gate
    grant_gate_create(&app_state, &actor.id).await;
    let gate = app_state
        .gate_manager
        .create_gate(
            &actor.id,
            "test-gate".to_string(),
            "name \"test-gate\"\nversion \"0.5.11\"\nbranch \"2024.0.0\"\npublisher \"test\""
                .to_string(),
        )
        .await
        .expect("Failed to create gate");

    assert_eq!(gate.name, "test-gate");
    assert_eq!(gate.owner_id, actor.id);

    // Verify Jujutsu repo exists
    let repo_path = format!("{}/gates/{}", app_state.settings.jj_repos.root, gate.id);
    assert!(
        std::path::Path::new(&repo_path).exists(),
        "Jujutsu repo should exist at {}",
        repo_path
    );

    // Cleanup
    app_state.shutdown().await.expect("Failed to shutdown");
    cleanup_test_data();
}

/// Test component creation with files
#[tokio::test]
#[ignore]
async fn test_component_with_files() {
    let app_state = setup_test_app_state().await;

    // Create actor and gate
    let actor = app_state
        .actor_repo
        .create_or_update_from_oidc("test_user".to_string(), "User".to_string())
        .await
        .expect("Failed to create actor");

    grant_gate_create(&app_state, &actor.id).await;
    let gate = app_state
        .gate_manager
        .create_gate(
            &actor.id,
            "component-test-gate".to_string(),
            "name \"component-test-gate\"\nversion \"0.5.11\"\nbranch \"2024.0.0\"\npublisher \"test\"".to_string(),
        )
        .await
        .expect("Failed to create gate");

    // Create component
    let component = app_state
        .component_manager
        .create_component(
            &actor.id,
            &gate.id,
            "test-component".to_string(),
            "name \"test-component\"\nsummary \"Test component\"\nversion \"1.0.0\"".to_string(),
        )
        .await
        .expect("Failed to create component");

    // Add source archive
    let archive_data = b"fake archive content".to_vec();
    let archive = app_state
        .component_manager
        .add_source_archive(
            &actor.id,
            &component.id,
            "test-archive.tar.gz".to_string(),
            Some("https://example.com/archive.tar.gz".to_string()),
            archive_data.clone(),
        )
        .await
        .expect("Failed to add source archive");

    assert_eq!(archive.filename, "test-archive.tar.gz");
    assert_eq!(archive.size_bytes, archive_data.len() as i64);

    // Get build manifest
    let manifest = app_state
        .component_manager
        .get_build_manifest(&actor.id, &component.id)
        .await
        .expect("Failed to get build manifest");

    assert_eq!(manifest.component_name, "test-component");
    assert_eq!(manifest.source_archives.len(), 1);

    // Cleanup
    app_state.shutdown().await.expect("Failed to shutdown");
    cleanup_test_data();
}

/// Test RBAC permissions
#[tokio::test]
#[ignore]
async fn test_rbac_enforcement() {
    let app_state = setup_test_app_state().await;

    // Create owner and member
    let owner = app_state
        .actor_repo
        .create_or_update_from_oidc("owner".to_string(), "Owner".to_string())
        .await
        .expect("Failed to create owner");

    let member = app_state
        .actor_repo
        .create_or_update_from_oidc("member".to_string(), "Member".to_string())
        .await
        .expect("Failed to create member");

    // Create gate
    grant_gate_create(&app_state, &owner.id).await;
    let gate = app_state
        .gate_manager
        .create_gate(
            &owner.id,
            "rbac-gate".to_string(),
            "name \"rbac-gate\"\nversion \"0.5.11\"\nbranch \"2024.0.0\"\npublisher \"test\""
                .to_string(),
        )
        .await
        .expect("Failed to create gate");

    // Member should not have access initially
    let has_read = app_state
        .rbac
        .check_gate_read(&member.id, &gate.id)
        .await
        .expect("Failed to check permission");
    assert!(!has_read, "Member should not have read access initially");

    // Add member with read permission
    app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &member.id,
            vec!["member".to_string()],
            vec!["gate_read".to_string(), "component_read".to_string()],
        )
        .await
        .expect("Failed to add member");

    // Member should now have read access
    let has_read = app_state
        .rbac
        .check_gate_read(&member.id, &gate.id)
        .await
        .expect("Failed to check permission");
    assert!(has_read, "Member should have read access after being added");

    // Member should not have write access
    let has_write = app_state
        .rbac
        .check_gate_write(&member.id, &gate.id)
        .await
        .expect("Failed to check permission");
    assert!(!has_write, "Member should not have write access");

    // Cleanup
    app_state.shutdown().await.expect("Failed to shutdown");
    cleanup_test_data();
}

// Helper functions

async fn grant_gate_create(app_state: &AppState, actor_id: &str) {
    let existing = server_member::Entity::find()
        .filter(server_member::Column::ActorId.eq(actor_id))
        .one(app_state.db.as_ref())
        .await
        .expect("Failed to query server_member");

    if existing.is_some() {
        return; // Already has a server_member entry
    }

    let now = chrono::Utc::now().fixed_offset();
    let active = server_member::ActiveModel {
        actor_id: Set(actor_id.to_string()),
        roles: Set(serde_json::json!(["gate_creator"])),
        permissions: Set(serde_json::json!(["gate_create"])),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    active
        .insert(app_state.db.as_ref())
        .await
        .expect("Failed to grant gate_create permission");
}

async fn setup_test_app_state() -> AppState {
    let settings = Settings {
        postgres: forged::settings::PostgresConfig {
            url: std::env::var("TEST_DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://forged:forged@localhost/forged_test".to_string()),
            max_connections: 5,
            ..Default::default()
        },
        seaweedfs: forged::settings::SeaweedFsConfig {
            master_url: std::env::var("TEST_SEAWEEDFS_URL")
                .unwrap_or_else(|_| "http://localhost:9333".to_string()),
            namespace: "test".to_string(),
            ..Default::default()
        },
        jj_repos: forged::settings::JjReposConfig {
            root: "./test_data/jj-repos".to_string(),
        },
        ..Default::default()
    };

    AppState::new(settings)
        .await
        .expect("Failed to create AppState")
}

fn cleanup_test_data() {
    // Remove test Jujutsu repos
    if std::path::Path::new("./test_data").exists() {
        let _ = std::fs::remove_dir_all("./test_data");
    }
}
