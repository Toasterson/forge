use forged::repositories::ApplicationBlobType;

mod common;
use common::TestContext;

#[tokio::test]
async fn test_blob_storage_and_retrieval() {
    let ctx = TestContext::new().await;

    let test_data = b"test blob content";

    // Store blob
    let (hash, fid) = ctx
        .app_state
        .blob_repo
        .store_blob(test_data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store blob");

    assert!(!hash.is_empty());
    assert!(!fid.is_empty());

    // Retrieve blob
    let retrieved = ctx
        .app_state
        .blob_repo
        .get_blob(&hash, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to retrieve blob");

    assert_eq!(retrieved, test_data);
}

#[tokio::test]
async fn test_blob_deduplication() {
    let ctx = TestContext::new().await;

    let test_data = b"duplicate content";

    // Store same blob twice
    let (hash1, fid1) = ctx
        .app_state
        .blob_repo
        .store_blob(test_data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store blob");

    let (hash2, fid2) = ctx
        .app_state
        .blob_repo
        .store_blob(test_data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store blob");

    // Same hash and FID (deduplication)
    assert_eq!(hash1, hash2);
    assert_eq!(fid1, fid2);
}

#[tokio::test]
async fn test_blob_not_found() {
    let ctx = TestContext::new().await;

    let result = ctx
        .app_state
        .blob_repo
        .get_blob("nonexistent_hash", ApplicationBlobType::SourceArchive)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_blob_exists() {
    let ctx = TestContext::new().await;

    let test_data = b"existence test";

    // Store blob
    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(test_data, ApplicationBlobType::Patch)
        .await
        .expect("Failed to store blob");

    // Check existence
    let exists = ctx
        .app_state
        .blob_repo
        .blob_exists(&hash, ApplicationBlobType::Patch)
        .await
        .expect("Failed to check blob existence");

    assert!(exists);

    // Check non-existent
    let not_exists = ctx
        .app_state
        .blob_repo
        .blob_exists("fake_hash", ApplicationBlobType::Patch)
        .await
        .expect("Failed to check blob existence");

    assert!(!not_exists);
}

#[tokio::test]
async fn test_list_blobs_by_type() {
    let ctx = TestContext::new().await;

    // Store blobs of different types
    ctx.app_state
        .blob_repo
        .store_blob(b"source 1", ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store");

    ctx.app_state
        .blob_repo
        .store_blob(b"source 2", ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store");

    ctx.app_state
        .blob_repo
        .store_blob(b"patch 1", ApplicationBlobType::Patch)
        .await
        .expect("Failed to store");

    // List source archives
    let source_archives = ctx
        .app_state
        .blob_repo
        .list_blobs(ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to list blobs");

    assert_eq!(source_archives.len(), 2);

    // List patches
    let patches = ctx
        .app_state
        .blob_repo
        .list_blobs(ApplicationBlobType::Patch)
        .await
        .expect("Failed to list blobs");

    assert_eq!(patches.len(), 1);
}

#[tokio::test]
async fn test_different_blob_types() {
    let ctx = TestContext::new().await;

    let test_data = b"multi-type content";

    // Store same data as different types
    let (hash_source, _) = ctx
        .app_state
        .blob_repo
        .store_blob(test_data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store");

    let (hash_patch, _) = ctx
        .app_state
        .blob_repo
        .store_blob(test_data, ApplicationBlobType::Patch)
        .await
        .expect("Failed to store");

    // Hashes should be the same (content-addressed)
    assert_eq!(hash_source, hash_patch);

    // Both should be retrievable by their type
    let retrieved_source = ctx
        .app_state
        .blob_repo
        .get_blob(&hash_source, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to retrieve source");

    let retrieved_patch = ctx
        .app_state
        .blob_repo
        .get_blob(&hash_patch, ApplicationBlobType::Patch)
        .await
        .expect("Failed to retrieve patch");

    assert_eq!(retrieved_source, test_data);
    assert_eq!(retrieved_patch, test_data);
}
