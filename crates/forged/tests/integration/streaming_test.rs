mod common;
use common::TestContext;
use forged::repositories::ApplicationBlobType;

#[tokio::test]
#[ignore] // Large file test - run manually or in CI with sufficient resources
async fn test_large_file_upload() {
    let ctx = TestContext::new().await;

    // Create 100MB file
    let large_data = vec![0xDE; 100 * 1024 * 1024];

    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(&large_data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to upload large file");

    // Verify can retrieve
    let retrieved = ctx
        .app_state
        .blob_repo
        .get_blob(&hash, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to download large file");

    assert_eq!(retrieved.len(), large_data.len());
}

#[tokio::test]
async fn test_medium_file_upload() {
    let ctx = TestContext::new().await;

    // Create 10MB file
    let data = vec![0xAB; 10 * 1024 * 1024];

    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(&data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store medium file");

    // Verify retrieval
    let retrieved = ctx
        .app_state
        .blob_repo
        .get_blob(&hash, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to retrieve medium file");

    assert_eq!(retrieved.len(), data.len());
    assert_eq!(retrieved, data);
}

#[tokio::test]
async fn test_multiple_large_archives() {
    let ctx = TestContext::new().await;

    use common::fixtures::TestFixtures;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "large-component").await;

    // Add 3 medium-sized archives (5MB each)
    for i in 0..3 {
        let archive_data = vec![i as u8; 5 * 1024 * 1024];
        ctx.app_state
            .component_manager
            .add_source_archive(
                &owner.id,
                &component.id,
                format!("archive-{}.tar.gz", i),
                None,
                archive_data,
            )
            .await
            .expect("Failed to add archive");
    }

    // Get manifest
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&owner.id, &component.id)
        .await
        .expect("Failed to get manifest");

    assert_eq!(manifest.source_archives.len(), 3);

    // Verify all archives can be downloaded
    for archive in &manifest.source_archives {
        let blob = ctx
            .app_state
            .blob_repo
            .get_blob(&archive.hash, ApplicationBlobType::SourceArchive)
            .await
            .expect("Failed to download archive");

        assert_eq!(blob.len(), 5 * 1024 * 1024);
    }
}

#[tokio::test]
async fn test_blob_integrity() {
    let ctx = TestContext::new().await;

    // Create data with specific pattern
    let mut data = Vec::with_capacity(1024 * 1024);
    for i in 0..(1024 * 1024) {
        data.push((i % 256) as u8);
    }

    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(&data, ApplicationBlobType::Patch)
        .await
        .expect("Failed to store blob");

    // Retrieve and verify pattern
    let retrieved = ctx
        .app_state
        .blob_repo
        .get_blob(&hash, ApplicationBlobType::Patch)
        .await
        .expect("Failed to retrieve blob");

    assert_eq!(retrieved.len(), data.len());

    // Verify pattern integrity
    for (i, byte) in retrieved.iter().enumerate() {
        assert_eq!(*byte, (i % 256) as u8, "Data corruption at byte {}", i);
    }
}

#[tokio::test]
async fn test_empty_blob() {
    let ctx = TestContext::new().await;

    let empty_data = vec![];

    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(&empty_data, ApplicationBlobType::License)
        .await
        .expect("Failed to store empty blob");

    let retrieved = ctx
        .app_state
        .blob_repo
        .get_blob(&hash, ApplicationBlobType::License)
        .await
        .expect("Failed to retrieve empty blob");

    assert_eq!(retrieved.len(), 0);
}

#[tokio::test]
async fn test_various_file_sizes() {
    let ctx = TestContext::new().await;

    // Test various sizes: 1B, 1KB, 100KB, 1MB, 5MB
    let sizes = vec![1, 1024, 100 * 1024, 1024 * 1024, 5 * 1024 * 1024];

    for size in sizes {
        let data = vec![0x42; size];

        let (hash, _) = ctx
            .app_state
            .blob_repo
            .store_blob(&data, ApplicationBlobType::Script)
            .await
            .expect(&format!("Failed to store {} byte blob", size));

        let retrieved = ctx
            .app_state
            .blob_repo
            .get_blob(&hash, ApplicationBlobType::Script)
            .await
            .expect(&format!("Failed to retrieve {} byte blob", size));

        assert_eq!(
            retrieved.len(),
            size,
            "Size mismatch for {} byte blob",
            size
        );
    }
}

#[tokio::test]
async fn test_concurrent_large_uploads() {
    let ctx = TestContext::new().await;

    use futures::future::join_all;

    // Upload 5 medium files concurrently (2MB each)
    let mut tasks = vec![];
    for i in 0..5 {
        let blob_repo = ctx.app_state.blob_repo.clone();
        let data = vec![i as u8; 2 * 1024 * 1024];

        tasks.push(tokio::spawn(async move {
            blob_repo
                .store_blob(&data, ApplicationBlobType::SourceArchive)
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

#[tokio::test]
async fn test_binary_data() {
    let ctx = TestContext::new().await;

    // Create binary data with all byte values
    let mut binary_data = Vec::with_capacity(256);
    for i in 0..=255u8 {
        binary_data.push(i);
    }

    // Repeat pattern to make it larger
    let large_binary = binary_data.repeat(1024);

    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(&large_binary, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store binary data");

    let retrieved = ctx
        .app_state
        .blob_repo
        .get_blob(&hash, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to retrieve binary data");

    assert_eq!(retrieved, large_binary);
}

#[tokio::test]
async fn test_compressed_archive_like_data() {
    let ctx = TestContext::new().await;

    use common::fixtures::TestFixtures;
    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Simulate a compressed tarball with mixed content
    let mut archive_data = Vec::new();

    // Header-like bytes
    archive_data.extend_from_slice(b"\x1f\x8b\x08\x00");

    // Mixed binary content
    for i in 0..1024 {
        archive_data.push((i % 256) as u8);
    }

    // Add some text-like content
    archive_data.extend_from_slice(b"This is a file inside the archive\n");

    // More binary
    archive_data.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);

    ctx.app_state
        .component_manager
        .add_source_archive(
            &owner.id,
            &component.id,
            "mixed-content.tar.gz".to_string(),
            None,
            archive_data.clone(),
        )
        .await
        .expect("Failed to add mixed content archive");

    // Retrieve and verify
    let manifest = ctx
        .app_state
        .component_manager
        .get_build_manifest(&owner.id, &component.id)
        .await
        .expect("Failed to get manifest");

    let retrieved = ctx
        .app_state
        .blob_repo
        .get_blob(
            &manifest.source_archives[0].hash,
            ApplicationBlobType::SourceArchive,
        )
        .await
        .expect("Failed to retrieve archive");

    assert_eq!(retrieved, archive_data);
}
