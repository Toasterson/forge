mod common;
use common::{fixtures::TestFixtures, TestContext};
use forged::repositories::ApplicationBlobType;
use forged::transport::grpc::proto::{
    build_service_server::BuildService, ActorRef, ComponentId, ContentHash, DownloadBlobRequest,
    GetBuildManifestRequest,
};
use forged::transport::BuildServiceImpl;
use futures::StreamExt;

fn actor_ref(id: String) -> ActorRef {
    ActorRef {
        id,
        kind: "user".to_string(),
    }
}

#[tokio::test]
async fn test_get_build_manifest() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    // Add source archive
    ctx.app_state
        .component_manager
        .add_source_archive(
            &owner.id,
            &component.id,
            "source.tar.gz".to_string(),
            None,
            b"source data".to_vec(),
        )
        .await
        .expect("Failed to add archive");

    let request = tonic::Request::new(GetBuildManifestRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
    });

    let response = build_service
        .get_build_manifest(request)
        .await
        .expect("Failed to get manifest");

    let manifest = response.into_inner().manifest.unwrap();

    assert_eq!(manifest.component_name, "test-component");
    assert_eq!(manifest.source_archives.len(), 1);
    assert_eq!(manifest.source_archives[0].filename, "source.tar.gz");
}

#[tokio::test]
async fn test_get_build_manifest_with_all_file_types() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "full-component").await;

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

    let request = tonic::Request::new(GetBuildManifestRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
    });

    let response = build_service
        .get_build_manifest(request)
        .await
        .expect("Failed to get manifest");

    let manifest = response.into_inner().manifest.unwrap();

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
async fn test_get_manifest_nonexistent_component() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    let request = tonic::Request::new(GetBuildManifestRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: "nonexistent".to_string(),
        }),
    });

    let result = build_service.get_build_manifest(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_download_blob() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    // Store blob
    let test_data = b"test blob content for download";
    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(test_data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store blob");

    let request = tonic::Request::new(DownloadBlobRequest {
        actor: Some(actor_ref(owner.id.clone())),
        hash: Some(ContentHash { hex: hash.clone() }),
        blob_type: "source_archive".to_string(),
    });

    let response = build_service
        .download_blob(request)
        .await
        .expect("Failed to start download");

    let mut stream = response.into_inner();

    // Collect all chunks
    let mut downloaded = Vec::new();
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.expect("Chunk error");
        downloaded.extend_from_slice(&chunk.chunk);
    }

    assert_eq!(downloaded, test_data);
}

#[tokio::test]
async fn test_download_blob_streaming() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    // Store 1MB blob (should be chunked)
    let large_data = vec![0xAB; 1024 * 1024];
    let (hash, _) = ctx
        .app_state
        .blob_repo
        .store_blob(&large_data, ApplicationBlobType::SourceArchive)
        .await
        .expect("Failed to store blob");

    let request = tonic::Request::new(DownloadBlobRequest {
        actor: Some(actor_ref(owner.id.clone())),
        hash: Some(ContentHash { hex: hash.clone() }),
        blob_type: "source_archive".to_string(),
    });

    let response = build_service
        .download_blob(request)
        .await
        .expect("Failed to start download");

    let mut stream = response.into_inner();

    // Collect all chunks and verify streaming
    let mut downloaded = Vec::new();
    let mut chunk_count = 0;
    let mut first_chunk = true;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.expect("Chunk error");

        // First chunk should contain total_size
        if first_chunk {
            assert_eq!(chunk.total_size, large_data.len() as i64);
            first_chunk = false;
        }

        downloaded.extend_from_slice(&chunk.chunk);
        chunk_count += 1;
    }

    assert_eq!(downloaded.len(), large_data.len());
    assert_eq!(downloaded, large_data);

    // Should have multiple chunks (1MB / 64KB > 1)
    assert!(chunk_count > 1, "Expected multiple chunks, got {}", chunk_count);
}

#[tokio::test]
async fn test_download_nonexistent_blob() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    let request = tonic::Request::new(DownloadBlobRequest {
        actor: Some(actor_ref(owner.id.clone())),
        hash: Some(ContentHash {
            hex: "nonexistent_hash".to_string(),
        }),
        blob_type: "source_archive".to_string(),
    });

    let result = build_service.download_blob(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_download_invalid_blob_type() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    let request = tonic::Request::new(DownloadBlobRequest {
        actor: Some(actor_ref(owner.id.clone())),
        hash: Some(ContentHash {
            hex: "some_hash".to_string(),
        }),
        blob_type: "invalid_type".to_string(),
    });

    let result = build_service.download_blob(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_download_different_blob_types() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    // Store blobs of different types
    let blob_types = vec![
        (b"source archive", ApplicationBlobType::SourceArchive, "source_archive"),
        (b"patch content", ApplicationBlobType::Patch, "patch"),
        (b"license text", ApplicationBlobType::License, "license"),
        (b"script code", ApplicationBlobType::Script, "script"),
    ];

    for (data, app_type, type_str) in blob_types {
        let (hash, _) = ctx
            .app_state
            .blob_repo
            .store_blob(data, app_type)
            .await
            .expect("Failed to store blob");

        let request = tonic::Request::new(DownloadBlobRequest {
            actor: Some(actor_ref(owner.id.clone())),
            hash: Some(ContentHash { hex: hash.clone() }),
            blob_type: type_str.to_string(),
        });

        let response = build_service
            .download_blob(request)
            .await
            .expect("Failed to download blob");

        let mut stream = response.into_inner();

        // Collect downloaded data
        let mut downloaded = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.expect("Chunk error");
            downloaded.extend_from_slice(&chunk.chunk);
        }

        assert_eq!(downloaded, data);
    }
}

#[tokio::test]
async fn test_empty_manifest() {
    let ctx = TestContext::new().await;
    let build_service = BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "empty-component").await;

    // Don't add any files

    let request = tonic::Request::new(GetBuildManifestRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
    });

    let response = build_service
        .get_build_manifest(request)
        .await
        .expect("Failed to get manifest");

    let manifest = response.into_inner().manifest.unwrap();

    assert_eq!(manifest.component_name, "empty-component");
    assert_eq!(manifest.source_archives.len(), 0);
    assert_eq!(manifest.patches.len(), 0);
    assert_eq!(manifest.licenses.len(), 0);
    assert_eq!(manifest.scripts.len(), 0);
}
