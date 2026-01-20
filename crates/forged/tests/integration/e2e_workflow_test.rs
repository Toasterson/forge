mod common;
use common::{fixtures::TestFixtures, TestContext};
use forged::repositories::ApplicationBlobType;

#[tokio::test]
async fn test_complete_component_build_workflow() {
    let ctx = TestContext::new().await;

    // 1. Create actors
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let developer = TestFixtures::actor(&ctx, "developer").await;

    // 2. Create gate
    let gate = TestFixtures::gate(&ctx, &owner, "production-gate").await;

    // 3. Add developer as member
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

    // 4. Developer creates component
    let component = ctx
        .app_state
        .component_manager
        .create_component(
            &developer.id,
            &gate.id,
            "my-package".to_string(),
            r#"component { name = "my-package" version = "1.0.0" }"#.to_string(),
        )
        .await
        .expect("Failed to create component");

    // 5. Upload source archive
    let archive_data = b"fake tarball content".to_vec();
    ctx.app_state
        .component_manager
        .add_source_archive(
            &developer.id,
            &component.id,
            "source-1.0.0.tar.gz".to_string(),
            Some("https://github.com/example/source/archive/1.0.0.tar.gz".to_string()),
            archive_data.clone(),
        )
        .await
        .expect("Failed to upload archive");

    // 6. Upload patches, licenses, scripts
    ctx.app_state
        .component_manager
        .add_component_file(
            &developer.id,
            &component.id,
            "patch",
            "security-fix.patch".to_string(),
            "patches/security-fix.patch".to_string(),
            b"diff --git a/main.c".to_vec(),
        )
        .await
        .expect("Failed to add patch");

    ctx.app_state
        .component_manager
        .add_component_file(
            &developer.id,
            &component.id,
            "license",
            "MIT".to_string(),
            "licenses/MIT".to_string(),
            b"MIT License\n\nPermission is hereby granted...".to_vec(),
        )
        .await
        .expect("Failed to add license");

    ctx.app_state
        .component_manager
        .add_component_file(
            &developer.id,
            &component.id,
            "script",
            "build.sh".to_string(),
            "scripts/build.sh".to_string(),
            b"#!/bin/bash\nmake install".to_vec(),
        )
        .await
        .expect("Failed to add script");

    // 7. Get build manifest
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&developer.id, &component.id)
        .await
        .expect("Failed to get manifest");

    assert_eq!(manifest.component_name, "my-package");
    assert_eq!(manifest.source_archives.len(), 1);
    assert_eq!(manifest.patches.len(), 1);
    assert_eq!(manifest.licenses.len(), 1);
    assert_eq!(manifest.scripts.len(), 1);

    // 8. Download each blob
    for archive in &manifest.source_archives {
        let blob = ctx
            .app_state
            .blob_repo
            .get_blob(&archive.hash, ApplicationBlobType::SourceArchive)
            .await
            .expect("Failed to download archive");
        assert!(!blob.is_empty());
    }

    for patch in &manifest.patches {
        let blob = ctx
            .app_state
            .blob_repo
            .get_blob(&patch.hash, ApplicationBlobType::Patch)
            .await
            .expect("Failed to download patch");
        assert!(!blob.is_empty());
    }

    for license in &manifest.licenses {
        let blob = ctx
            .app_state
            .blob_repo
            .get_blob(&license.hash, ApplicationBlobType::License)
            .await
            .expect("Failed to download license");
        assert!(!blob.is_empty());
    }

    for script in &manifest.scripts {
        let blob = ctx
            .app_state
            .blob_repo
            .get_blob(&script.hash, ApplicationBlobType::Script)
            .await
            .expect("Failed to download script");
        assert!(!blob.is_empty());
    }
}

#[tokio::test]
async fn test_multi_user_collaboration() {
    let ctx = TestContext::new().await;

    // Create owner and team members
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let dev1 = TestFixtures::actor(&ctx, "dev1").await;
    let dev2 = TestFixtures::actor(&ctx, "dev2").await;
    let reviewer = TestFixtures::actor(&ctx, "reviewer").await;

    // Create gate
    let gate = TestFixtures::gate(&ctx, &owner, "team-gate").await;

    // Add team members with different roles
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &dev1.id,
            vec!["developer".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
                "component_write".to_string(),
            ],
        )
        .await
        .expect("Failed to add dev1");

    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &dev2.id,
            vec!["developer".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
                "component_write".to_string(),
            ],
        )
        .await
        .expect("Failed to add dev2");

    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &reviewer.id,
            vec!["reviewer".to_string()],
            vec![
                "gate_read".to_string(),
                "component_read".to_string(),
            ],
        )
        .await
        .expect("Failed to add reviewer");

    // Dev1 creates component
    let component = TestFixtures::component(&ctx, &dev1, &gate, "team-component").await;

    // Dev2 adds source archive
    ctx.app_state
        .component_manager
        .add_source_archive(
            &dev2.id,
            &component.id,
            "source.tar.gz".to_string(),
            None,
            b"source code".to_vec(),
        )
        .await
        .expect("Failed to add archive");

    // Dev1 adds patch
    ctx.app_state
        .component_manager
        .add_component_file(
            &dev1.id,
            &component.id,
            "patch",
            "fix.patch".to_string(),
            "patches/fix.patch".to_string(),
            b"patch content".to_vec(),
        )
        .await
        .expect("Failed to add patch");

    // Reviewer can read but not write
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&reviewer.id, &component.id)
        .await
        .expect("Reviewer should be able to read manifest");

    assert_eq!(manifest.source_archives.len(), 1);
    assert_eq!(manifest.patches.len(), 1);

    // Reviewer cannot add files
    let result = ctx
        .app_state
        .component_manager
        .add_component_file(
            &reviewer.id,
            &component.id,
            "license",
            "LICENSE".to_string(),
            "licenses/LICENSE".to_string(),
            b"license".to_vec(),
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_component_version_workflow() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "versioned-gate").await;

    // Create component v1.0
    let component = ctx
        .app_state
        .component_manager
        .create_component(
            &owner.id,
            &gate.id,
            "versioned-package".to_string(),
            r#"component { version = "1.0" }"#.to_string(),
        )
        .await
        .expect("Failed to create component");

    // Add v1.0 source
    ctx.app_state
        .component_manager
        .add_source_archive(
            &owner.id,
            &component.id,
            "source-1.0.tar.gz".to_string(),
            None,
            b"v1.0 source".to_vec(),
        )
        .await
        .expect("Failed to add v1.0 source");

    // Update to v2.0
    ctx.app_state
        .component_manager
        .update_component(
            &owner.id,
            &component.id,
            r#"component { version = "2.0" }"#.to_string(),
        )
        .await
        .expect("Failed to update to v2.0");

    // Add v2.0 source
    ctx.app_state
        .component_manager
        .add_source_archive(
            &owner.id,
            &component.id,
            "source-2.0.tar.gz".to_string(),
            None,
            b"v2.0 source".to_vec(),
        )
        .await
        .expect("Failed to add v2.0 source");

    // Get manifest - should have both versions
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&owner.id, &component.id)
        .await
        .expect("Failed to get manifest");

    assert_eq!(manifest.source_archives.len(), 2);
}

#[tokio::test]
async fn test_gate_transfer_and_cleanup() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let new_admin = TestFixtures::actor(&ctx, "new_admin").await;
    let gate = TestFixtures::gate(&ctx, &owner, "transfer-gate").await;

    // Create some components
    for i in 1..=3 {
        TestFixtures::component(&ctx, &owner, &gate, &format!("component-{}", i)).await;
    }

    // Add new admin
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &new_admin.id,
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
        .expect("Failed to add new admin");

    // New admin should have full access
    let components = ctx
        .app_state
        .component_manager
        .list_components(&new_admin.id, &gate.id)
        .await
        .expect("Failed to list components");

    assert_eq!(components.len(), 3);

    // New admin can create components
    let new_component = ctx
        .app_state
        .component_manager
        .create_component(
            &new_admin.id,
            &gate.id,
            "admin-component".to_string(),
            "component { }".to_string(),
        )
        .await
        .expect("Failed to create component as new admin");

    assert_eq!(new_component.name, "admin-component");
}
