use forged::api::forged::api::v1 as pb;
use forged::services::{
    AuthServiceImpl, ComponentServiceImpl, GateServiceImpl, GitServiceImpl, SharedState,
};
use forged::settings::{GitStorageConfig, SurrealConfig};
use forged::storage::git::RepoManager;
use forged::storage::surreal as sdb;
use forged_client::api::forged::api::v1 as cpb;
use forged_client::ForgedClient;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

// Helper to start a forged gRPC server on an ephemeral port with temp storage.
async fn start_test_server(
    db_path: &std::path::Path,
    repos_root: &std::path::Path,
) -> miette::Result<(SocketAddr, tokio::sync::oneshot::Sender<()>)> {
    // Connect to embedded SurrealDB at the given path
    let surreal_cfg = SurrealConfig {
        mode: Some("embedded".into()),
        endpoint: None,
        username: None,
        password: None,
        namespace: Some("forged".into()),
        database: Some("default".into()),
        path: Some(db_path.to_string_lossy().to_string()),
    };
    let db = sdb::connect_from_config(&surreal_cfg).await?;

    // Generate ephemeral server SSH keys (same approach as State::default)
    let mut rng = ssh_key::rand_core::OsRng;
    let priv_key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519)
        .expect("generate ssh key");
    let public = priv_key.public_key();
    let private_key_ssh = priv_key
        .to_openssh(Default::default())
        .expect("encode openssh private")
        .to_string();
    let public_key_ssh = public.to_openssh().expect("encode openssh public");

    // Repo manager rooted at temp dir
    let repo_cfg = GitStorageConfig {
        mode: Some("fs".into()),
        root: Some(repos_root.to_string_lossy().to_string()),
        s3: None,
    };
    let repo_manager = RepoManager::new(repo_cfg);

    let shared = SharedState::new(
        db,
        private_key_ssh,
        public_key_ssh,
        None,
        None,
        repo_manager,
    );

    // Instantiate services
    let gate_impl = GateServiceImpl::from_shared(shared.clone());
    let component_impl = ComponentServiceImpl::from_shared(shared.clone());
    let auth_impl = AuthServiceImpl::from_shared(shared.clone());
    let git_impl = GitServiceImpl::from_shared(shared);

    // Bind listener on localhost, port 0
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    let incoming = TcpListenerStream::new(listener);

    let (tx, rx) = tokio::sync::oneshot::channel();

    // Spawn server
    tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(pb::gate_service_server::GateServiceServer::new(gate_impl))
            .add_service(pb::component_service_server::ComponentServiceServer::new(
                component_impl,
            ))
            .add_service(pb::auth_service_server::AuthServiceServer::new(auth_impl))
            .add_service(pb::git_service_server::GitServiceServer::new(git_impl))
            .serve_with_incoming_shutdown(incoming, async move {
                let _ = rx.await;
            })
            .await;
    });

    Ok((addr, tx))
}

fn temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("forge-test-{}-{}", prefix, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).expect("create temp dir");
    p
}

#[tokio::test]
async fn e2e_git_push_fetch_and_put_version_updates_metadata() -> miette::Result<()> {
    // Prepare temp storage locations
    let db_dir = temp_dir("db");
    let repos_dir = temp_dir("repos");

    // Start server
    let (addr, shutdown_tx) = start_test_server(&db_dir, &repos_dir).await?;
    let endpoint = format!("http://{}", addr);

    // Connect client
    let mut client = ForgedClient::connect(&endpoint).await?;

    // 1) Register/Add actor key for authentication
    // Use ed25519 seed for deterministic keypair
    use ed25519_dalek::{SigningKey, VerifyingKey};
    let seed = [7u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let vk: VerifyingKey = sk.verifying_key();
    // Store actor key
    let actor_id = "test-user@example.com".to_string();
    let key_id = "k1".to_string();
    let pk_rec = cpb::PublicKey {
        key_id: key_id.clone(),
        algorithm: "ed25519".into(),
        public_key: vk.as_bytes().to_vec(),
    };
    let add_req = cpb::AddActorKeyRequest {
        actor_id: actor_id.clone(),
        actor_kind: pb::ActorKind::User as i32,
        public_key: Some(pk_rec),
        proof: None,
    };
    let _ = client
        .auth
        .add_actor_key(tonic::Request::new(add_req))
        .await
        .expect("add actor key");

    // 2) Create a component and repo
    let component_id = "com.example.hello";
    let comp = cpb::Component {
        id: component_id.into(),
        name: "hello".into(),
        files: Some(cpb::ComponentFiles {
            patches: vec![],
            licenses: vec![],
            scripts: vec![],
        }),
        base_json: String::new(),
    };
    let _ = client
        .component
        .create_component(tonic::Request::new(cpb::CreateComponentRequest {
            component: Some(comp),
        }))
        .await
        .expect("create component");

    let _created = client.create_repo(component_id).await?;

    // 3) SmartPush a small fake packfile (must end with 20-byte trailer)
    let mut fake_pack: Vec<u8> = Vec::new();
    fake_pack
        .extend_from_slice(b"PACK\x00\x00\x00\x02garbage data that simulates a git pack file...");
    // Append 20-byte trailer checksum (arbitrary for our current server logic)
    fake_pack.extend_from_slice(&[0xAA; 20]);

    let proof_msg = b"forge-push-proof-v1";
    client
        .push_pack_ed25519(
            component_id,
            &actor_id,
            forged_client::AuthKind::User,
            &key_id,
            &seed,
            proof_msg,
            &mut &fake_pack[..],
        )
        .await?;

    // 4) SmartFetch and assert we got the same bytes back
    let fetched = client.fetch_latest_pack_bytes(component_id).await?;
    assert_eq!(fetched.len(), fake_pack.len());
    assert_eq!(&fetched[fetched.len() - 20..], &[0xAA; 20]);

    // 5) PutVersion with package.kdl and verify Surreal metadata updated
    let pkg_kdl = br#"component "hello" { version "1.2.3" }"#;
    let commit_id = client
        .put_version(component_id, "1.2.3", &pkg_kdl[..])
        .await?;
    assert!(!commit_id.is_empty());

    // Connect directly to the embedded Surreal to inspect the record
    let surreal_cfg = SurrealConfig {
        mode: Some("embedded".into()),
        endpoint: None,
        username: None,
        password: None,
        namespace: Some("forged".into()),
        database: Some("default".into()),
        path: Some(db_dir.to_string_lossy().to_string()),
    };
    let db2 = sdb::connect_from_config(&surreal_cfg).await?;
    let comp_id = forged::types::ComponentId(component_id.to_string());
    if let Some(rec) = sdb::get_component(&db2, &comp_id).await? {
        let meta = rec.metadata.unwrap_or(serde_json::json!({}));
        let current = meta
            .get("current_version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(current, "1.2.3");
        let kdl = meta
            .get("package_kdl")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(kdl.contains("component \"hello\""));
    } else {
        panic!("component not found in surreal");
    }

    // Shutdown server
    let _ = shutdown_tx.send(());

    Ok(())
}
