mod common;
use common::{fixtures::TestFixtures, TestContext};

#[tokio::test]
async fn test_component_creation_with_permissions() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let developer = TestFixtures::actor(&ctx, "developer").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

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
                "component_write".to_string(),
            ],
        )
        .await
        .expect("Failed to add developer");

    // Developer should be able to create component
    let component = ctx
        .app_state
        .component_manager
        .create_component(
            &developer.id,
            &gate.id,
            "dev-component".to_string(),
            "component { }".to_string(),
        )
        .await
        .expect("Failed to create component");

    assert_eq!(component.name, "dev-component");
}

#[tokio::test]
async fn test_get_component_requires_read_permission() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let stranger = TestFixtures::actor(&ctx, "stranger").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Stranger cannot read component
    let result = ctx
        .app_state
        .component_manager
        .get_component(&stranger.id, &component.id)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_component_requires_write_permission() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let reader = TestFixtures::actor(&ctx, "reader").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Add reader with only read permission
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &reader.id,
            vec!["reader".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
            ],
        )
        .await
        .expect("Failed to add reader");

    // Reader cannot update component
    let result = ctx
        .app_state
        .component_manager
        .update_component(
            &reader.id,
            &component.id,
            "component { version = \"2.0\" }".to_string(),
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_add_source_archive_with_permissions() {
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
        .expect("Failed to add developer");

    // Developer should be able to add source archive
    let archive = ctx
        .app_state
        .component_manager
        .add_source_archive(
            &developer.id,
            &component.id,
            "source.tar.gz".to_string(),
            None,
            b"archive data".to_vec(),
        )
        .await
        .expect("Failed to add archive");

    assert_eq!(archive.filename, "source.tar.gz");
}

#[tokio::test]
async fn test_build_manifest_with_all_file_types() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component =
        TestFixtures::component(&ctx, &owner, &gate, "full-component").await;

    // Add source archive
    ctx.app_state
        .component_manager
        .add_source_archive(
            &owner.id,
            &component.id,
            "source-1.0.0.tar.gz".to_string(),
            Some("https://example.com/source-1.0.0.tar.gz".to_string()),
            b"source archive content".to_vec(),
        )
        .await
        .expect("Failed to add source archive");

    // Add patch
    ctx.app_state
        .component_manager
        .add_component_file(
            &owner.id,
            &component.id,
            "patch",
            "security-fix.patch".to_string(),
            "patches/security-fix.patch".to_string(),
            b"diff --git a/main.c".to_vec(),
        )
        .await
        .expect("Failed to add patch");

    // Add license
    ctx.app_state
        .component_manager
        .add_component_file(
            &owner.id,
            &component.id,
            "license",
            "MIT".to_string(),
            "licenses/MIT".to_string(),
            b"MIT License...".to_vec(),
        )
        .await
        .expect("Failed to add license");

    // Add script
    ctx.app_state
        .component_manager
        .add_component_file(
            &owner.id,
            &component.id,
            "script",
            "build.sh".to_string(),
            "scripts/build.sh".to_string(),
            b"#!/bin/bash\nmake install".to_vec(),
        )
        .await
        .expect("Failed to add script");

    // Get build manifest
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&owner.id, &component.id)
        .await
        .expect("Failed to get manifest");

    // Verify all file types are present
    assert_eq!(manifest.component_name, "full-component");
    assert_eq!(manifest.source_archives.len(), 1);
    assert_eq!(manifest.patches.len(), 1);
    assert_eq!(manifest.licenses.len(), 1);
    assert_eq!(manifest.scripts.len(), 1);

    // Verify source archive details
    assert_eq!(manifest.source_archives[0].filename, "source-1.0.0.tar.gz");
    assert_eq!(
        manifest.source_archives[0].url,
        Some("https://example.com/source-1.0.0.tar.gz".to_string())
    );
}

#[tokio::test]
async fn test_multiple_patches() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "patched-component").await;

    // Add multiple patches
    for i in 1..=3 {
        ctx.app_state
            .component_manager
            .add_component_file(
                &owner.id,
                &component.id,
                "patch",
                format!("patch-{}.patch", i),
                format!("patches/patch-{}.patch", i),
                format!("diff --git a/file{}.c", i).into_bytes(),
            )
            .await
            .expect("Failed to add patch");
    }

    // Get manifest
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&owner.id, &component.id)
        .await
        .expect("Failed to get manifest");

    assert_eq!(manifest.patches.len(), 3);
}

#[tokio::test]
async fn test_get_manifest_requires_read_permission() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let stranger = TestFixtures::actor(&ctx, "stranger").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Stranger cannot get build manifest
    let result = ctx
        .app_state
        .component_manager
        .get_build_manifest(&stranger.id, &component.id)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_components_in_gate() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Create multiple components
    for i in 1..=5 {
        TestFixtures::component(&ctx, &owner, &gate, &format!("component-{}", i)).await;
    }

    // List components
    let components = ctx
        .app_state
        .component_manager
        .list_components(&owner.id, &gate.id)
        .await
        .expect("Failed to list components");

    assert_eq!(components.len(), 5);
}
