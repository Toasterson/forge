mod common;
use common::{fixtures::TestFixtures, TestContext};
use futures::future::join_all;

#[tokio::test]
async fn test_concurrent_component_creation() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Create 10 components concurrently
    let mut tasks = vec![];
    for i in 0..10 {
        let app_state = ctx.app_state.clone();
        let owner_id = owner.id.clone();
        let gate_id = gate.id.clone();

        tasks.push(tokio::spawn(async move {
            app_state
                .component_manager
                .create_component(
                    &owner_id,
                    &gate_id,
                    format!("component-{}", i),
                    format!("component {{ name = \"component-{}\" }}", i),
                )
                .await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    // Verify all components were created
    let components = ctx
        .app_state
        .component_manager
        .list_components(&owner.id, &gate.id)
        .await
        .expect("Failed to list components");

    assert_eq!(components.len(), 10);
}

#[tokio::test]
async fn test_concurrent_blob_uploads() {
    let ctx = TestContext::new().await;

    // Upload 20 different blobs concurrently
    let mut tasks = vec![];
    for i in 0..20 {
        let blob_repo = ctx.app_state.blob_repo.clone();
        let data = format!("blob content {}", i).into_bytes();

        tasks.push(tokio::spawn(async move {
            use forged::repositories::ApplicationBlobType;
            blob_repo
                .store_blob(&data, ApplicationBlobType::SourceArchive)
                .await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed and have unique hashes
    let mut hashes = std::collections::HashSet::new();
    for result in results {
        let (hash, _) = result.unwrap().expect("Blob upload failed");
        assert!(hashes.insert(hash), "Duplicate hash detected");
    }

    assert_eq!(hashes.len(), 20);
}

#[tokio::test]
async fn test_concurrent_same_blob_uploads() {
    let ctx = TestContext::new().await;

    let same_data = b"identical blob content";

    // Upload same blob 10 times concurrently (test deduplication)
    let mut tasks = vec![];
    for _ in 0..10 {
        let blob_repo = ctx.app_state.blob_repo.clone();
        let data = same_data.to_vec();

        tasks.push(tokio::spawn(async move {
            use forged::repositories::ApplicationBlobType;
            blob_repo
                .store_blob(&data, ApplicationBlobType::Patch)
                .await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed
    let mut hashes = std::collections::HashSet::new();
    for result in results {
        let (hash, _) = result.unwrap().expect("Blob upload failed");
        hashes.insert(hash);
    }

    // All should have the same hash (deduplication)
    assert_eq!(hashes.len(), 1);
}

#[tokio::test]
async fn test_concurrent_member_additions() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Create 5 members
    let mut members = vec![];
    for i in 0..5 {
        members.push(TestFixtures::actor(&ctx, &format!("member{}", i)).await);
    }

    // Add all members concurrently
    let mut tasks = vec![];
    for (i, member) in members.iter().enumerate() {
        let gate_manager = ctx.app_state.gate_manager.clone();
        let owner_id = owner.id.clone();
        let gate_id = gate.id.clone();
        let member_id = member.id.clone();

        tasks.push(tokio::spawn(async move {
            gate_manager
                .add_member(
                    &owner_id,
                    &gate_id,
                    &member_id,
                    vec![format!("role{}", i)],
                    vec!["gate_read".to_string()],
                )
                .await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    // Verify all members were added
    let gate_members = ctx
        .app_state
        .gate_manager
        .list_members(&owner.id, &gate.id)
        .await
        .expect("Failed to list members");

    assert_eq!(gate_members.len(), 5);
}

#[tokio::test]
async fn test_concurrent_file_uploads_to_component() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Upload different file types concurrently
    let mut tasks = vec![];

    // 3 patches
    for i in 0..3 {
        let component_manager = ctx.app_state.component_manager.clone();
        let owner_id = owner.id.clone();
        let component_id = component.id.clone();

        tasks.push(tokio::spawn(async move {
            component_manager
                .add_component_file(
                    &owner_id,
                    &component_id,
                    "patch",
                    format!("patch-{}.patch", i),
                    format!("patches/patch-{}.patch", i),
                    format!("diff {}", i).into_bytes(),
                )
                .await
        }));
    }

    // 2 licenses
    for i in 0..2 {
        let component_manager = ctx.app_state.component_manager.clone();
        let owner_id = owner.id.clone();
        let component_id = component.id.clone();

        tasks.push(tokio::spawn(async move {
            component_manager
                .add_component_file(
                    &owner_id,
                    &component_id,
                    "license",
                    format!("LICENSE-{}", i),
                    format!("licenses/LICENSE-{}", i),
                    format!("license {}", i).into_bytes(),
                )
                .await
        }));
    }

    // 2 scripts
    for i in 0..2 {
        let component_manager = ctx.app_state.component_manager.clone();
        let owner_id = owner.id.clone();
        let component_id = component.id.clone();

        tasks.push(tokio::spawn(async move {
            component_manager
                .add_component_file(
                    &owner_id,
                    &component_id,
                    "script",
                    format!("script-{}.sh", i),
                    format!("scripts/script-{}.sh", i),
                    format!("#!/bin/bash\necho {}", i).into_bytes(),
                )
                .await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    // Verify manifest
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&owner.id, &component.id)
        .await
        .expect("Failed to get manifest");

    assert_eq!(manifest.patches.len(), 3);
    assert_eq!(manifest.licenses.len(), 2);
    assert_eq!(manifest.scripts.len(), 2);
}

#[tokio::test]
async fn test_concurrent_reads_and_writes() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Add initial source archive
    ctx.app_state
        .component_manager
        .add_source_archive(
            &owner.id,
            &component.id,
            "initial.tar.gz".to_string(),
            None,
            b"initial content".to_vec(),
        )
        .await
        .expect("Failed to add initial archive");

    // Concurrently:
    // - 5 readers getting the manifest
    // - 3 writers adding patches
    let mut tasks = vec![];

    // Readers
    for _ in 0..5 {
        let component_manager = ctx.app_state.component_manager.clone();
        let owner_id = owner.id.clone();
        let component_id = component.id.clone();

        tasks.push(tokio::spawn(async move {
            component_manager
                .get_build_manifest(&owner_id, &component_id)
                .await
        }));
    }

    // Writers
    for i in 0..3 {
        let component_manager = ctx.app_state.component_manager.clone();
        let owner_id = owner.id.clone();
        let component_id = component.id.clone();

        tasks.push(tokio::spawn(async move {
            component_manager
                .add_component_file(
                    &owner_id,
                    &component_id,
                    "patch",
                    format!("concurrent-{}.patch", i),
                    format!("patches/concurrent-{}.patch", i),
                    format!("patch {}", i).into_bytes(),
                )
                .await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    // Final manifest should have all patches
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&owner.id, &component.id)
        .await
        .expect("Failed to get manifest");

    assert_eq!(manifest.patches.len(), 3);
}

#[tokio::test]
async fn test_concurrent_gate_operations() {
    let ctx = TestContext::new().await;
    let owner = TestFixtures::actor(&ctx, "owner").await;

    // Create multiple gates concurrently
    let mut tasks = vec![];
    for i in 0..5 {
        let gate_manager = ctx.app_state.gate_manager.clone();
        let owner_id = owner.id.clone();

        tasks.push(tokio::spawn(async move {
            gate_manager
                .create_gate(
                    &owner_id,
                    format!("gate-{}", i),
                    format!("gate {{ name = \"gate-{}\" }}", i),
                )
                .await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }
}
