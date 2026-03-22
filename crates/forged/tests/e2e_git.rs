// End-to-end tests for the Forge V2 gRPC API.
//
// These tests start an actual gRPC server on a random port and exercise
// the full request path: transport → middleware → service → repository → storage.
//
// Requirements:
//   - PostgreSQL running (TEST_DATABASE_URL)
//   - SeaweedFS running (TEST_SEAWEEDFS_URL)
//   - RabbitMQ running (for build dispatch tests)
//
// Run with: cargo test --test e2e_git -- --ignored --nocapture

mod common;

use common::fixtures::TestFixtures;
use common::TestContext;
use forged::settings::TlsMode;
use forged::transport;
use forged::transport::grpc::proto::{
    auth_service_client::AuthServiceClient, build_service_client::BuildServiceClient,
    component_service_client::ComponentServiceClient, gate_service_client::GateServiceClient,
    ActorRef, AddMemberRequest, AuthenticateRequest, ComponentId, CreateComponentRequest,
    CreateGateRequest, GateId, GetBuildManifestRequest, GetComponentRequest, GetGateRequest,
    ListComponentFilesRequest, ListComponentsRequest, ListGatesRequest, ListMembersRequest,
    ListSourceArchivesRequest, RemoveMemberRequest, UpdateComponentRequest, UpdateGateRequest,
};
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;

/// Start a test gRPC server on a random port, returning the address and a cancel token.
async fn start_test_server(ctx: &TestContext) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    drop(listener); // Release the port for tonic to use

    let cancel = CancellationToken::new();

    // Clone all Arc references before moving into the spawn
    let auth_service = transport::AuthServiceImpl::new(ctx.app_state.auth.clone());
    let gate_service = transport::GateServiceImpl::new(
        ctx.app_state.gate_repo.clone(),
        ctx.app_state.component_repo.clone(),
        ctx.app_state.rbac.clone(),
    );
    let component_service = transport::ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.rbac.clone(),
    )
    .with_max_upload_size(10 * 1024 * 1024); // 10 MiB for tests
    let build_service = transport::BuildServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
        ctx.app_state.blob_repo.clone(),
        ctx.app_state.rbac.clone(),
        ctx.app_state.build_dispatch.clone(),
    );
    let oidc = ctx.app_state.oidc.clone();
    let actor_repo = ctx.app_state.actor_repo.clone();

    tokio::spawn(async move {
        let tls_config = forged::settings::TlsConfig {
            mode: TlsMode::None,
            ..Default::default()
        };
        transport::start_grpc_server(
            addr,
            auth_service,
            gate_service,
            component_service,
            build_service,
            oidc,
            actor_repo,
            &tls_config,
            None, // No health check deps in tests
            1000,
        )
        .await
        .expect("gRPC server failed");
    });

    // Give the server a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    (addr, cancel)
}

fn actor_ref(id: &str) -> Option<ActorRef> {
    Some(ActorRef {
        id: id.to_string(),
        kind: "user".to_string(),
    })
}

fn gate_id(id: &str) -> Option<GateId> {
    Some(GateId { id: id.to_string() })
}

fn component_id(id: &str) -> Option<ComponentId> {
    Some(ComponentId { id: id.to_string() })
}

// =============================================================================
// Auth Service E2E Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_authenticate_and_get_actor() {
    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let mut client = AuthServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    // Authenticate with stub token (OIDC issuer_url is empty = stub mode)
    let resp = client
        .authenticate(AuthenticateRequest {
            oidc_token: "test_user:Alice".to_string(),
        })
        .await
        .expect("authenticate");

    let inner = resp.into_inner();
    let actor = inner.actor.expect("actor present");
    assert!(!actor.id.is_empty());
    assert_eq!(actor.kind, "user");
    assert_eq!(inner.display_name, "Alice");
}

#[tokio::test]
#[ignore]
async fn e2e_authenticate_idempotent() {
    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let mut client = AuthServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    // Same sub, different display name should update
    let resp1 = client
        .authenticate(AuthenticateRequest {
            oidc_token: "sub_42:Name1".to_string(),
        })
        .await
        .expect("auth 1");
    let id1 = resp1.into_inner().actor.unwrap().id;

    let resp2 = client
        .authenticate(AuthenticateRequest {
            oidc_token: "sub_42:Name2".to_string(),
        })
        .await
        .expect("auth 2");
    let inner2 = resp2.into_inner();
    let id2 = inner2.actor.unwrap().id;

    assert_eq!(id1, id2, "same sub should yield same actor ID");
    assert_eq!(inner2.display_name, "Name2", "display name should update");
}

// =============================================================================
// Gate Service E2E Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_gate_lifecycle() {
    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    // Create actor via repository (middleware bypass -- stub OIDC has no real token flow)
    let actor = TestFixtures::actor(&ctx, "gate_owner").await;

    let mut gate_client = GateServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    // Inject auth by creating a request with the actor's OIDC sub as bearer token.
    // The middleware validates via OidcService stub mode (empty issuer = stub).
    let mut req = tonic::Request::new(CreateGateRequest {
        actor: actor_ref(&actor.id),
        name: "e2e-test-gate".to_string(),
        gate_kdl: r#"name "e2e-test-gate" version "0.5.11" branch "2024.0.0" publisher "test""#
            .to_string(),
    });
    req.metadata_mut().insert(
        "authorization",
        format!(
            "Bearer {}:{}",
            actor.oidc_sub.as_deref().unwrap_or(""),
            actor.display_name
        )
        .parse()
        .unwrap(),
    );

    let create_resp = gate_client.create_gate(req).await.expect("create gate");
    let gate_info = create_resp.into_inner().gate.expect("gate present");
    assert_eq!(gate_info.name, "e2e-test-gate");
    assert!(!gate_info.id.is_empty());
    let created_gate_id = gate_info.id.clone();

    // GetGate
    let mut req = tonic::Request::new(GetGateRequest {
        actor: actor_ref(&actor.id),
        gate_id: gate_id(&created_gate_id),
    });
    req.metadata_mut().insert(
        "authorization",
        format!(
            "Bearer {}:{}",
            actor.oidc_sub.as_deref().unwrap_or(""),
            actor.display_name
        )
        .parse()
        .unwrap(),
    );
    let get_resp = gate_client.get_gate(req).await.expect("get gate");
    let fetched = get_resp.into_inner().gate.unwrap();
    assert_eq!(fetched.id, created_gate_id);
    assert_eq!(fetched.name, "e2e-test-gate");

    // UpdateGate
    let mut req = tonic::Request::new(UpdateGateRequest {
        actor: actor_ref(&actor.id),
        gate_id: gate_id(&created_gate_id),
        gate_kdl: r#"name "e2e-test-gate" version "0.5.11" branch "2024.0.1" publisher "test""#
            .to_string(),
    });
    req.metadata_mut().insert(
        "authorization",
        format!(
            "Bearer {}:{}",
            actor.oidc_sub.as_deref().unwrap_or(""),
            actor.display_name
        )
        .parse()
        .unwrap(),
    );
    let update_resp = gate_client.update_gate(req).await.expect("update gate");
    let updated = update_resp.into_inner().gate.unwrap();
    assert!(updated.gate_kdl.contains("2024.0.1"));

    // ListGates
    let mut req = tonic::Request::new(ListGatesRequest {
        actor: actor_ref(&actor.id),
        owner_id: Some(actor.id.clone()),
        page_size: 0,
        page_token: String::new(),
    });
    req.metadata_mut().insert(
        "authorization",
        format!(
            "Bearer {}:{}",
            actor.oidc_sub.as_deref().unwrap_or(""),
            actor.display_name
        )
        .parse()
        .unwrap(),
    );
    let list_resp = gate_client.list_gates(req).await.expect("list gates");
    let gates = list_resp.into_inner().gates;
    assert!(!gates.is_empty(), "should find at least one gate");
    assert!(gates.iter().any(|g| g.id == created_gate_id));
}

// =============================================================================
// Gate Membership E2E Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_gate_membership() {
    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let owner = TestFixtures::actor(&ctx, "membership_owner").await;
    let member = TestFixtures::actor(&ctx, "membership_member").await;
    let gate = TestFixtures::gate(&ctx, &owner, "membership-gate").await;

    let mut client = GateServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    let bearer = format!(
        "Bearer {}:{}",
        owner.oidc_sub.as_deref().unwrap_or(""),
        owner.display_name
    );

    // AddMember
    let mut req = tonic::Request::new(AddMemberRequest {
        actor: actor_ref(&owner.id),
        gate_id: gate_id(&gate.id),
        member_actor_id: member.id.clone(),
        roles: vec!["member".to_string()],
        permissions: vec!["gate_read".to_string(), "component_read".to_string()],
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let add_resp = client.add_member(req).await.expect("add member");
    let member_info = add_resp.into_inner().member.unwrap();
    assert_eq!(member_info.actor_id, member.id);

    // ListMembers
    let mut req = tonic::Request::new(ListMembersRequest {
        actor: actor_ref(&owner.id),
        gate_id: gate_id(&gate.id),
        page_size: 0,
        page_token: String::new(),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let list_resp = client.list_members(req).await.expect("list members");
    let members = list_resp.into_inner().members;
    assert!(
        members.iter().any(|m| m.actor_id == member.id),
        "member should appear in list"
    );

    // RemoveMember
    let mut req = tonic::Request::new(RemoveMemberRequest {
        actor: actor_ref(&owner.id),
        gate_id: gate_id(&gate.id),
        member_actor_id: member.id.clone(),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let remove_resp = client.remove_member(req).await.expect("remove member");
    assert!(remove_resp.into_inner().success);
}

// =============================================================================
// Component Service E2E Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_component_lifecycle() {
    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let actor = TestFixtures::actor(&ctx, "comp_owner").await;
    let gate = TestFixtures::gate(&ctx, &actor, "comp-gate").await;

    let mut client = ComponentServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    let bearer = format!(
        "Bearer {}:{}",
        actor.oidc_sub.as_deref().unwrap_or(""),
        actor.display_name
    );

    // CreateComponent
    let recipe = r#"name "library/test-lib"
summary "A test library"
version "1.0.0"
source {
    archive "https://example.com/test-1.0.0.tar.gz" sha256="abc123"
}
build {
    configure {
        option "--prefix=/usr"
    }
}"#;

    let mut req = tonic::Request::new(CreateComponentRequest {
        actor: actor_ref(&actor.id),
        gate_id: gate_id(&gate.id),
        name: "library/test-lib".to_string(),
        recipe_kdl: recipe.to_string(),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());

    let create_resp = client
        .create_component(req)
        .await
        .expect("create component");
    let comp = create_resp.into_inner().component.unwrap();
    assert_eq!(comp.name, "library/test-lib");
    assert!(!comp.id.is_empty());
    let comp_id = comp.id.clone();

    // GetComponent
    let mut req = tonic::Request::new(GetComponentRequest {
        actor: actor_ref(&actor.id),
        component_id: component_id(&comp_id),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let get_resp = client.get_component(req).await.expect("get component");
    let fetched = get_resp.into_inner().component.unwrap();
    assert_eq!(fetched.id, comp_id);
    assert!(fetched.recipe_kdl.contains("library/test-lib"));

    // UpdateComponent
    let updated_recipe = recipe.replace("1.0.0", "2.0.0");
    let mut req = tonic::Request::new(UpdateComponentRequest {
        actor: actor_ref(&actor.id),
        component_id: component_id(&comp_id),
        recipe_kdl: updated_recipe,
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let update_resp = client
        .update_component(req)
        .await
        .expect("update component");
    let updated = update_resp.into_inner().component.unwrap();
    assert!(updated.recipe_kdl.contains("2.0.0"));

    // ListComponents via GateService
    let mut gate_client = GateServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect gate");
    let mut req = tonic::Request::new(ListComponentsRequest {
        actor: actor_ref(&actor.id),
        gate_id: gate_id(&gate.id),
        page_size: 0,
        page_token: String::new(),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let list_resp = gate_client
        .list_components(req)
        .await
        .expect("list components");
    let components = list_resp.into_inner().components;
    assert!(components.iter().any(|c| c.id == comp_id));
}

// =============================================================================
// File Upload E2E Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_upload_source_archive() {
    use forged::transport::grpc::proto::{
        upload_source_archive_request, ArchiveMetadata, UploadSourceArchiveRequest,
    };

    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let actor = TestFixtures::actor(&ctx, "upload_owner").await;
    let gate = TestFixtures::gate(&ctx, &actor, "upload-gate").await;
    let comp = TestFixtures::component(&ctx, &actor, &gate, "upload-comp").await;

    let mut client = ComponentServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    let bearer = format!(
        "Bearer {}:{}",
        actor.oidc_sub.as_deref().unwrap_or(""),
        actor.display_name
    );

    // Create fake archive data
    let archive_data = vec![0xDE; 4096]; // 4KB

    // Build streaming request: metadata first, then chunks
    let metadata_msg = UploadSourceArchiveRequest {
        data: Some(upload_source_archive_request::Data::Metadata(
            ArchiveMetadata {
                actor: actor_ref(&actor.id),
                component_id: component_id(&comp.id),
                filename: "test-archive-1.0.tar.gz".to_string(),
                url: Some("https://example.com/test-archive-1.0.tar.gz".to_string()),
                total_size: archive_data.len() as i64,
            },
        )),
    };

    // Split data into chunks
    let chunk_size = 1024;
    let mut messages = vec![metadata_msg];
    for chunk in archive_data.chunks(chunk_size) {
        messages.push(UploadSourceArchiveRequest {
            data: Some(upload_source_archive_request::Data::Chunk(chunk.to_vec())),
        });
    }

    let stream = tokio_stream::iter(messages);
    let mut req = tonic::Request::new(stream);
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());

    let resp = client
        .upload_source_archive(req)
        .await
        .expect("upload source archive");
    let archive_info = resp.into_inner().archive.unwrap();
    assert_eq!(archive_info.filename, "test-archive-1.0.tar.gz");
    assert_eq!(archive_info.size_bytes, 4096);
    assert!(archive_info.hash.is_some());
    let hash = archive_info.hash.unwrap();
    assert!(!hash.hex.is_empty(), "hash should be computed");

    // Verify via ListSourceArchives
    let mut req = tonic::Request::new(ListSourceArchivesRequest {
        actor: actor_ref(&actor.id),
        component_id: component_id(&comp.id),
        page_size: 0,
        page_token: String::new(),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let list_resp = client
        .list_source_archives(req)
        .await
        .expect("list archives");
    let archives = list_resp.into_inner().archives;
    assert_eq!(archives.len(), 1);
    assert_eq!(archives[0].filename, "test-archive-1.0.tar.gz");
}

#[tokio::test]
#[ignore]
async fn e2e_upload_component_file() {
    use forged::transport::grpc::proto::{
        upload_component_file_request, FileMetadata, UploadComponentFileRequest,
    };

    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let actor = TestFixtures::actor(&ctx, "file_owner").await;
    let gate = TestFixtures::gate(&ctx, &actor, "file-gate").await;
    let comp = TestFixtures::component(&ctx, &actor, &gate, "file-comp").await;

    let mut client = ComponentServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    let bearer = format!(
        "Bearer {}:{}",
        actor.oidc_sub.as_deref().unwrap_or(""),
        actor.display_name
    );

    // Upload a patch file
    let patch_data = b"--- a/Makefile\n+++ b/Makefile\n@@ -1 +1 @@\n-OLD\n+NEW\n".to_vec();

    let metadata_msg = UploadComponentFileRequest {
        data: Some(upload_component_file_request::Data::Metadata(
            FileMetadata {
                actor: actor_ref(&actor.id),
                component_id: component_id(&comp.id),
                kind: "patch".to_string(),
                name: "fix-makefile.patch".to_string(),
                rel_path: "patches/fix-makefile.patch".to_string(),
                total_size: patch_data.len() as i64,
            },
        )),
    };

    let chunk_msg = UploadComponentFileRequest {
        data: Some(upload_component_file_request::Data::Chunk(
            patch_data.clone(),
        )),
    };

    let stream = tokio_stream::iter(vec![metadata_msg, chunk_msg]);
    let mut req = tonic::Request::new(stream);
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());

    let resp = client
        .upload_component_file(req)
        .await
        .expect("upload component file");
    let file_info = resp.into_inner().file.unwrap();
    assert_eq!(file_info.kind, "patch");
    assert_eq!(file_info.name, "fix-makefile.patch");
    assert_eq!(file_info.size_bytes, patch_data.len() as i64);

    // Verify via ListComponentFiles filtered by kind
    let mut req = tonic::Request::new(ListComponentFilesRequest {
        actor: actor_ref(&actor.id),
        component_id: component_id(&comp.id),
        kind: Some("patch".to_string()),
        page_size: 0,
        page_token: String::new(),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    let list_resp = client.list_component_files(req).await.expect("list files");
    let files = list_resp.into_inner().files;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "fix-makefile.patch");
}

// =============================================================================
// Build Service E2E Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_build_manifest() {
    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let actor = TestFixtures::actor(&ctx, "build_owner").await;
    let gate = TestFixtures::gate(&ctx, &actor, "build-gate").await;
    let comp = TestFixtures::component(&ctx, &actor, &gate, "build-comp").await;

    // Upload a source archive via the repository layer (faster for setup)
    let archive_data = b"fake-archive-content-for-build-test";
    ctx.app_state
        .source_archive_repo
        .add_source_archive(
            &comp.id,
            "source-1.0.tar.gz".to_string(),
            Some("https://example.com/source-1.0.tar.gz".to_string()),
            archive_data,
        )
        .await
        .expect("add archive");

    let mut client = BuildServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    let bearer = format!(
        "Bearer {}:{}",
        actor.oidc_sub.as_deref().unwrap_or(""),
        actor.display_name
    );

    // GetBuildManifest
    let mut req = tonic::Request::new(GetBuildManifestRequest {
        actor: actor_ref(&actor.id),
        component_id: component_id(&comp.id),
    });
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());

    let resp = client
        .get_build_manifest(req)
        .await
        .expect("get build manifest");
    let manifest = resp.into_inner().manifest.unwrap();
    assert_eq!(manifest.component_name, "build-comp");
    assert!(!manifest.recipe_kdl.is_empty());
    assert_eq!(manifest.source_archives.len(), 1);
    assert_eq!(manifest.source_archives[0].filename, "source-1.0.tar.gz");
}

// =============================================================================
// Health Check E2E Test
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_health_check() {
    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    // Use the generated health check client from forged's proto
    // tonic-health's HealthClient requires the "transport" feature which may not
    // be available as a direct client. Instead, verify via a simple TCP connection
    // that the server is listening and accepting connections.
    let stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("should connect to gRPC server");
    assert!(stream.peer_addr().is_ok(), "should have valid peer addr");
}

// =============================================================================
// Full Workflow E2E Test
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_full_packaging_workflow() {
    // This test exercises the complete packaging workflow:
    // 1. Authenticate
    // 2. Create a gate
    // 3. Create a component
    // 4. Upload source archive
    // 5. Upload a patch file
    // 6. Get build manifest (should include archive + patch)
    // 7. Verify pagination works on list endpoints

    use forged::transport::grpc::proto::{
        upload_component_file_request, upload_source_archive_request, ArchiveMetadata,
        FileMetadata, UploadComponentFileRequest, UploadSourceArchiveRequest,
    };

    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let endpoint = format!("http://{}", addr);

    let mut auth = AuthServiceClient::connect(endpoint.clone()).await.unwrap();
    let mut gates = GateServiceClient::connect(endpoint.clone()).await.unwrap();
    let mut components = ComponentServiceClient::connect(endpoint.clone())
        .await
        .unwrap();
    let mut builds = BuildServiceClient::connect(endpoint.clone()).await.unwrap();

    // 1. Authenticate
    let auth_resp = auth
        .authenticate(AuthenticateRequest {
            oidc_token: "workflow_user:Workflow User".to_string(),
        })
        .await
        .unwrap();
    let actor_id = auth_resp.into_inner().actor.unwrap().id;
    let bearer: tonic::metadata::MetadataValue<_> =
        "Bearer workflow_user:Workflow User".parse().unwrap();

    // 2. Create gate
    let mut req = tonic::Request::new(CreateGateRequest {
        actor: actor_ref(&actor_id),
        name: "workflow-gate".to_string(),
        gate_kdl: r#"name "workflow-gate" version "0.5.11" branch "2024.0.0" publisher "test.com""#
            .to_string(),
    });
    req.metadata_mut().insert("authorization", bearer.clone());
    let gate_resp = gates.create_gate(req).await.unwrap();
    let gid = gate_resp.into_inner().gate.unwrap().id;

    // 3. Create component
    let mut req = tonic::Request::new(CreateComponentRequest {
        actor: actor_ref(&actor_id),
        gate_id: gate_id(&gid),
        name: "web/curl".to_string(),
        recipe_kdl: r#"name "web/curl"
summary "The CURL Network Utility"
version "8.6.0"
source {
    archive "https://curl.haxx.se/download/curl-8.6.0.tar.xz" sha256="abc123"
    patch "fix-configure.patch"
}
build {
    configure {
        option "--prefix=/usr"
    }
}
dependency "library/zlib" kind="require"
"#
        .to_string(),
    });
    req.metadata_mut().insert("authorization", bearer.clone());
    let comp_resp = components.create_component(req).await.unwrap();
    let cid = comp_resp.into_inner().component.unwrap().id;

    // 4. Upload source archive
    let archive_data = vec![0x50; 4096]; // 4KB fake archive
    let stream = tokio_stream::iter(vec![
        UploadSourceArchiveRequest {
            data: Some(upload_source_archive_request::Data::Metadata(
                ArchiveMetadata {
                    actor: actor_ref(&actor_id),
                    component_id: component_id(&cid),
                    filename: "curl-8.6.0.tar.xz".to_string(),
                    url: Some("https://curl.haxx.se/download/curl-8.6.0.tar.xz".to_string()),
                    total_size: archive_data.len() as i64,
                },
            )),
        },
        UploadSourceArchiveRequest {
            data: Some(upload_source_archive_request::Data::Chunk(archive_data)),
        },
    ]);
    let mut req = tonic::Request::new(stream);
    req.metadata_mut().insert("authorization", bearer.clone());
    components
        .upload_source_archive(req)
        .await
        .expect("upload archive");

    // 5. Upload patch file
    let patch_data = b"--- a/configure\n+++ b/configure\n".to_vec();
    let stream = tokio_stream::iter(vec![
        UploadComponentFileRequest {
            data: Some(upload_component_file_request::Data::Metadata(
                FileMetadata {
                    actor: actor_ref(&actor_id),
                    component_id: component_id(&cid),
                    kind: "patch".to_string(),
                    name: "fix-configure.patch".to_string(),
                    rel_path: "patches/fix-configure.patch".to_string(),
                    total_size: patch_data.len() as i64,
                },
            )),
        },
        UploadComponentFileRequest {
            data: Some(upload_component_file_request::Data::Chunk(patch_data)),
        },
    ]);
    let mut req = tonic::Request::new(stream);
    req.metadata_mut().insert("authorization", bearer.clone());
    components
        .upload_component_file(req)
        .await
        .expect("upload patch");

    // 6. Get build manifest
    let mut req = tonic::Request::new(GetBuildManifestRequest {
        actor: actor_ref(&actor_id),
        component_id: component_id(&cid),
    });
    req.metadata_mut().insert("authorization", bearer.clone());
    let manifest_resp = builds.get_build_manifest(req).await.unwrap();
    let manifest = manifest_resp.into_inner().manifest.unwrap();
    assert_eq!(manifest.component_name, "web/curl");
    assert_eq!(manifest.source_archives.len(), 1, "should have 1 archive");
    assert_eq!(manifest.source_archives[0].filename, "curl-8.6.0.tar.xz");
    assert_eq!(manifest.patches.len(), 1, "should have 1 patch");
    assert_eq!(manifest.patches[0].name, "fix-configure.patch");

    // 7. Verify pagination on list endpoints
    let mut req = tonic::Request::new(ListGatesRequest {
        actor: actor_ref(&actor_id),
        owner_id: None,
        page_size: 1,
        page_token: String::new(),
    });
    req.metadata_mut().insert("authorization", bearer.clone());
    let list_resp = gates.list_gates(req).await.unwrap();
    let inner = list_resp.into_inner();
    assert_eq!(inner.gates.len(), 1);
    // next_page_token should be empty if there's only one gate
    // (may be non-empty if other tests have left data, but the point is it compiles and works)
}

// =============================================================================
// Upload Size Limit E2E Test
// =============================================================================

#[tokio::test]
#[ignore]
async fn e2e_upload_size_limit_enforced() {
    use forged::transport::grpc::proto::{
        upload_source_archive_request, ArchiveMetadata, UploadSourceArchiveRequest,
    };

    let ctx = TestContext::new().await;
    let (addr, _cancel) = start_test_server(&ctx).await;

    let actor = TestFixtures::actor(&ctx, "limit_owner").await;
    let gate = TestFixtures::gate(&ctx, &actor, "limit-gate").await;
    let comp = TestFixtures::component(&ctx, &actor, &gate, "limit-comp").await;

    let mut client = ComponentServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("connect");

    let bearer = format!(
        "Bearer {}:{}",
        actor.oidc_sub.as_deref().unwrap_or(""),
        actor.display_name
    );

    // Try to upload a file that exceeds the 10 MiB test limit
    // by declaring a total_size larger than the limit
    let stream = tokio_stream::iter(vec![UploadSourceArchiveRequest {
        data: Some(upload_source_archive_request::Data::Metadata(
            ArchiveMetadata {
                actor: actor_ref(&actor.id),
                component_id: component_id(&comp.id),
                filename: "too-big.tar.gz".to_string(),
                url: None,
                total_size: 100 * 1024 * 1024, // 100 MiB -- exceeds 10 MiB test limit
            },
        )),
    }]);

    let mut req = tonic::Request::new(stream);
    req.metadata_mut()
        .insert("authorization", bearer.parse().unwrap());

    let result = client.upload_source_archive(req).await;
    assert!(result.is_err(), "should reject oversized upload");
    let status = result.unwrap_err();
    assert_eq!(
        status.code(),
        tonic::Code::ResourceExhausted,
        "should be resource exhausted: {}",
        status.message()
    );
}
