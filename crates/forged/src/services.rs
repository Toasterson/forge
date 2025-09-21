use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use lettre::AsyncTransport;
use lettre::{message::Mailbox, AsyncSmtpTransport, Message, Tokio1Executor};
// SurrealDB storage
use crate::storage::git::RepoManager;
use crate::storage::surreal as sdb;
use serde::{Deserialize, Serialize};
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

use crate::api::forged::api::v1 as api;
use age::{Decryptor, Encryptor};
use std::io::{BufReader, Cursor, Read, Write};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct GateServiceImpl {
    state: Arc<State>,
}

impl Default for GateServiceImpl {
    fn default() -> Self {
        Self {
            state: Arc::new(State::default()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComponentServiceImpl {
    state: Arc<State>,
}

impl Default for ComponentServiceImpl {
    fn default() -> Self {
        Self {
            state: Arc::new(State::default()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthServiceImpl {
    state: Arc<State>,
}

#[derive(Clone)]
pub struct SharedState(Arc<State>);

impl SharedState {
    pub fn new(
        db: sdb::Db,
        private_ssh: String,
        public_ssh: String,
        mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
        mail_from: Option<String>,
        repo_manager: RepoManager,
    ) -> Self {
        let state = State {
            surreal: Some(db),
            server_private_ssh: private_ssh,
            server_public_ssh: public_ssh,
            mailer,
            mail_from,
            repo_manager: Some(repo_manager),
            ..Default::default()
        };
        SharedState(Arc::new(state))
    }
}

impl GateServiceImpl {
    pub fn from_shared(shared: SharedState) -> Self {
        Self {
            state: shared.0.clone(),
        }
    }
}

impl ComponentServiceImpl {
    pub fn from_shared(shared: SharedState) -> Self {
        Self {
            state: shared.0.clone(),
        }
    }
}

#[tonic::async_trait]
impl api::git_service_server::GitService for GitServiceImpl {
    type SmartPushStream =
        tokio_stream::wrappers::ReceiverStream<Result<api::SmartPushResponse, Status>>;
    type SmartFetchStream =
        tokio_stream::wrappers::ReceiverStream<Result<api::SmartFetchResponse, Status>>;

    async fn create_repo(
        &self,
        request: Request<api::CreateRepoRequest>,
    ) -> Result<Response<api::CreateRepoResponse>, Status> {
        let Some(repo) = &self.state.repo_manager else {
            return Err(Status::failed_precondition("repo manager not available"));
        };
        let req = request.into_inner();
        if req.component_id.is_empty() {
            return Err(Status::invalid_argument("component_id is required"));
        }
        match repo.ensure_repo(&req.component_id) {
            Ok(created) => Ok(Response::new(api::CreateRepoResponse { created })),
            Err(e) => {
                error!(component_id=%req.component_id, error=?e, "ensure repo failed");
                Err(Status::internal("create repo failed"))
            }
        }
    }

    async fn put_version(
        &self,
        request: Request<api::PutVersionRequest>,
    ) -> Result<Response<api::PutVersionResponse>, Status> {
        let req = request.into_inner();
        if req.component_id.is_empty() {
            return Err(Status::invalid_argument("component_id is required"));
        }
        if req.version.is_empty() {
            return Err(Status::invalid_argument("version is required"));
        }
        let Some(repo) = &self.state.repo_manager else {
            return Err(Status::failed_precondition("repo manager not available"));
        };
        // Commit package.kdl to the component repo
        let commit_id = repo
            .put_version_package_kdl(&req.component_id, &req.version, &req.package_kdl)
            .map_err(|e| {
                error!(component_id=%req.component_id, version=%req.version, error=?e, "put version failed");
                Status::internal("put version failed")
            })?;

        // Update SurrealDB: set current package.kdl contents as latest in metadata
        if let Some(db) = &self.state.surreal {
            let comp_id = crate::types::ComponentId(req.component_id.clone());
            match sdb::get_component(db, &comp_id).await {
                Ok(Some(mut rec)) => {
                    // Store KDL as plain string under metadata.package_kdl
                    let kdl_str = String::from_utf8_lossy(&req.package_kdl).to_string();
                    let mut meta = rec.metadata.take().unwrap_or_else(|| serde_json::json!({}));
                    if let Some(obj) = meta.as_object_mut() {
                        obj.insert("package_kdl".into(), serde_json::Value::String(kdl_str));
                        obj.insert(
                            "current_version".into(),
                            serde_json::Value::String(req.version.clone()),
                        );
                    }
                    rec.metadata = Some(meta);
                    rec.touch();
                    if let Err(e) = sdb::put_component(db, &rec).await {
                        error!(component_id=%req.component_id, error=?e, "update component current package.kdl failed");
                    }
                }
                Ok(None) => {
                    warn!(component_id=%req.component_id, "component not found when updating current package.kdl");
                }
                Err(e) => {
                    error!(component_id=%req.component_id, error=?e, "surreal get component failed");
                }
            }
        }

        Ok(Response::new(api::PutVersionResponse { commit_id }))
    }

    async fn smart_push(
        &self,
        request: Request<tonic::Streaming<api::SmartPushRequest>>,
    ) -> Result<Response<Self::SmartPushStream>, Status> {
        use tokio::io::AsyncWriteExt;
        use tokio::sync::mpsc;

        let Some(repo_mgr) = &self.state.repo_manager else {
            return Err(Status::failed_precondition("repo manager not available"));
        };

        // Receive first message and validate it's an `open` with auth
        let mut inbound = request.into_inner();
        let first = inbound
            .message()
            .await
            .map_err(|e| Status::internal(format!("receive stream error: {e}")))?;
        let Some(first_msg) = first else {
            return Err(Status::invalid_argument("empty stream"));
        };
        let open = match first_msg.payload {
            Some(api::smart_push_request::Payload::Open(o)) => o,
            _ => return Err(Status::invalid_argument("first message must be `open`")),
        };
        if open.component_id.is_empty() || open.actor_id.is_empty() || open.key_id.is_empty() {
            return Err(Status::invalid_argument(
                "open.component_id, actor_id and key_id are required",
            ));
        }

        // Authenticate using stored actor key
        let db = self
            .state
            .surreal
            .as_ref()
            .ok_or_else(|| Status::failed_precondition("database not available for auth"))?;
        let key = sdb::get_actor_key(db, &open.actor_id, &open.key_id)
            .await
            .map_err(|e| {
                error!(actor_id=%open.actor_id, key_id=%open.key_id, error=?e, "get actor key failed");
                Status::internal("auth lookup failed")
            })?
            .ok_or_else(|| Status::unauthenticated("actor key not found"))?;
        // ed25519 verification (if algorithm matches)
        if open.algorithm.eq_ignore_ascii_case("ed25519") {
            if let Some(proof) = open.proof {
                if !crate::services::verify_ed25519(
                    &key.public_key,
                    &proof.message,
                    &proof.signature,
                ) {
                    return Err(Status::unauthenticated("signature verification failed"));
                }
            } else {
                return Err(Status::unauthenticated("missing proof"));
            }
        } else {
            return Err(Status::unauthenticated("unsupported key algorithm"));
        }

        // Ensure a bare repository exists for push ingestion
        if let Err(e) = repo_mgr.ensure_bare_repo(&open.component_id) {
            error!(component_id=%open.component_id, error=?e, "ensure bare repo failed");
            return Err(Status::internal("failed to ensure bare repository"));
        }
        let bare_dir = repo_mgr.repo_bare_dir(&open.component_id);

        // Prepare incoming pack sink path
        let pack_dir = bare_dir.join("objects").join("pack");
        if let Err(e) = std::fs::create_dir_all(&pack_dir) {
            error!(dir=%pack_dir.display(), error=?e, "create pack dir failed");
            return Err(Status::internal("failed to prepare pack directory"));
        }
        let ts = now_sec();
        let pack_path = pack_dir.join(format!("incoming-{ts}.pack"));
        let file = match tokio::fs::File::create(&pack_path).await {
            Ok(f) => f,
            Err(e) => {
                error!(path=%pack_path.display(), error=?e, "create pack file failed");
                return Err(Status::internal("failed to create pack file"));
            }
        };
        let file = tokio::sync::Mutex::new(file);

        let (tx, rx) = mpsc::channel(16);
        let tx_progress = tx.clone();

        // clone inputs needed in the writer task
        let repo_mgr2 = repo_mgr.clone();
        let component_id = open.component_id.clone();
        let pack_path2 = pack_path.clone();

        // Writer task: append all packfile chunks to the file
        tokio::spawn(async move {
            let mut wrote: u64 = 0;

            // Process remaining messages
            while let Ok(maybe) = inbound.message().await {
                let Some(msg) = maybe else {
                    break;
                };
                match msg.payload {
                    Some(api::smart_push_request::Payload::PackfileChunk(bytes)) => {
                        let mut guard = file.lock().await;
                        if let Err(e) = guard.write_all(&bytes).await {
                            let _ = tx_progress
                                .send(Err(Status::internal(format!("pack write failed: {e}"))))
                                .await;
                            return;
                        }
                        wrote += bytes.len() as u64;
                        let _ = tx_progress
                            .send(Ok(api::SmartPushResponse {
                                payload: Some(api::smart_push_response::Payload::Progress(
                                    format!("received {} bytes", wrote),
                                )),
                            }))
                            .await;
                    }
                    Some(api::smart_push_request::Payload::Done(true)) => break,
                    _ => {}
                }
            }

            // Flush and close the file before finalization
            {
                let mut guard = file.lock().await;
                if let Err(e) = guard.flush().await {
                    let _ = tx_progress
                        .send(Err(Status::internal(format!("flush pack failed: {e}"))))
                        .await;
                    return;
                }
            }

            // Finalize: rename incoming pack to canonical pack-<hash>.pack
            match repo_mgr2.finalize_incoming_pack(&component_id, &pack_path2) {
                Ok(final_path) => {
                    let _ = tx_progress
                        .send(Ok(api::SmartPushResponse {
                            payload: Some(api::smart_push_response::Payload::Progress(format!(
                                "stored pack as {}",
                                final_path
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("(unknown)")
                            ))),
                        }))
                        .await;
                    let _ = tx_progress
                        .send(Ok(api::SmartPushResponse {
                            payload: Some(api::smart_push_response::Payload::Accepted(true)),
                        }))
                        .await;
                }
                Err(e) => {
                    let _ = tx_progress
                        .send(Err(Status::internal(format!("finalize pack failed: {e}"))))
                        .await;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn smart_fetch(
        &self,
        request: Request<api::SmartFetchRequest>,
    ) -> Result<Response<Self::SmartFetchStream>, Status> {
        use tokio::io::AsyncReadExt;
        use tokio::sync::mpsc;

        let req = request.into_inner();
        if req.component_id.is_empty() {
            return Err(Status::invalid_argument("component_id is required"));
        }
        let Some(repo_mgr) = &self.state.repo_manager else {
            return Err(Status::failed_precondition("repo manager not available"));
        };

        // Try to find the latest pack file in the bare repository
        let pack_path = match repo_mgr.latest_pack_path(&req.component_id) {
            Ok(p) => p,
            Err(e) => {
                warn!(component_id=%req.component_id, error=?e, "no pack available for fetch");
                return Err(Status::not_found("no pack available for repository"));
            }
        };

        // Open file for async reading
        let file = match tokio::fs::File::open(&pack_path).await {
            Ok(f) => f,
            Err(e) => {
                error!(path=%pack_path.display(), error=?e, "open pack for fetch failed");
                return Err(Status::internal("failed to open pack"));
            }
        };
        let mut reader = file;

        let (tx, rx) = mpsc::channel(16);
        let path_str = pack_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("pack.pack")
            .to_string();

        tokio::spawn(async move {
            let mut _sent: u64 = 0;
            // Send initial progress
            let _ = tx
                .send(Ok(api::SmartFetchResponse {
                    payload: Some(api::smart_fetch_response::Payload::Progress(format!(
                        "streaming {}",
                        path_str
                    ))),
                }))
                .await;

            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        _sent += n as u64;
                        let _ = tx
                            .send(Ok(api::SmartFetchResponse {
                                payload: Some(api::smart_fetch_response::Payload::PackfileChunk(
                                    buf[..n].to_vec(),
                                )),
                            }))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Ok(api::SmartFetchResponse {
                                payload: Some(api::smart_fetch_response::Payload::Error(format!(
                                    "read error: {}",
                                    e
                                ))),
                            }))
                            .await;
                        return;
                    }
                }
            }
            let _ = tx
                .send(Ok(api::SmartFetchResponse {
                    payload: Some(api::smart_fetch_response::Payload::Done(true)),
                }))
                .await;
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

impl AuthServiceImpl {
    pub fn from_shared(shared: SharedState) -> Self {
        Self {
            state: shared.0.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitServiceImpl {
    state: Arc<State>,
}

impl GitServiceImpl {
    pub fn from_shared(shared: SharedState) -> Self {
        Self {
            state: shared.0.clone(),
        }
    }
}

impl AuthServiceImpl {
    pub fn with_surreal_keys_and_mailer(
        db: sdb::Db,
        private_ssh: String,
        public_ssh: String,
        mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
        mail_from: Option<String>,
    ) -> Self {
        let state = State {
            surreal: Some(db),
            server_private_ssh: private_ssh,
            server_public_ssh: public_ssh,
            mailer,
            mail_from,
            ..Default::default()
        };
        Self {
            state: Arc::new(state),
        }
    }

    pub fn with_keys(private_ssh: String, public_ssh: String) -> Self {
        let state = State {
            server_private_ssh: private_ssh,
            server_public_ssh: public_ssh,
            ..Default::default()
        };
        Self {
            state: Arc::new(state),
        }
    }

    pub fn with_keys_and_mailer(
        private_ssh: String,
        public_ssh: String,
        mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
        mail_from: Option<String>,
    ) -> Self {
        let state = State {
            server_private_ssh: private_ssh,
            server_public_ssh: public_ssh,
            mailer,
            mail_from,
            ..Default::default()
        };
        Self {
            state: Arc::new(state),
        }
    }
}

impl Default for AuthServiceImpl {
    fn default() -> Self {
        Self {
            state: Arc::new(State::default()),
        }
    }
}

#[derive(Debug)]
struct State {
    pending: Mutex<HashMap<String, PendingRegistration>>, // key: actor_key(actor_id, kind)
    surreal: Option<sdb::Db>,                             // present when using SurrealDB backend
    server_private_ssh: String,
    server_public_ssh: String,
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    mail_from: Option<String>,
    repo_manager: Option<RepoManager>,
}

impl Default for State {
    fn default() -> Self {
        // Generate ephemeral Ed25519 SSH keypair for tests/defaults
        let mut rng = ssh_key::rand_core::OsRng;
        let priv_key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519)
            .expect("generate ssh key");
        let public = priv_key.public_key();
        let private_key_ssh = priv_key
            .to_openssh(Default::default())
            .expect("encode openssh private")
            .to_string();
        let public_key_ssh = public.to_openssh().expect("encode openssh public");
        Self {
            pending: Mutex::new(HashMap::new()),
            surreal: None,
            server_private_ssh: private_key_ssh,
            server_public_ssh: public_key_ssh,
            mailer: None,
            repo_manager: None,
            mail_from: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRegistration {
    #[serde(rename = "_id")]
    id: String,
    actor_id: String,
    actor_kind: api::ActorKind,
    public_key: Option<api::PublicKey>,
    expires_at: u64,
    envelope: Vec<u8>, // what we "sent by email"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfirmationMeta {
    actor_id: String,
    actor_kind: String,
    issued_at: u64,
    expires_at: u64,
    key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfirmationPayload {
    nonce: String,
    server_key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfirmationEnvelope {
    meta: ConfirmationMeta,
    // inner payload is "encrypted" with the server key. For demo purposes, we base64 the JSON string.
    payload_ciphertext_b64: String,
    // the whole envelope is meant to be encrypted for the actor. For demo, we do not apply real crypto,
    // but include algorithm hints so clients know what to expect in future.
    encrypted_for_algorithm: String,
}

fn actor_key(actor_id: &str, kind: api::ActorKind) -> String {
    format!("{}::{:?}", actor_id, kind)
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---- Gate type conversions between API and server models ----
fn permission_to_str(p: &crate::rbac::Permission) -> String {
    // Use serde's snake_case via JSON string, stripping quotes
    serde_json::to_string(p)
        .unwrap_or("\"unknown\"".into())
        .trim_matches('"')
        .to_string()
}

fn permission_from_str(s: &str) -> Option<crate::rbac::Permission> {
    let quoted = format!("\"{}\"", s);
    serde_json::from_str::<crate::rbac::Permission>(&quoted).ok()
}

fn api_actor_to_types(a: &api::ActorRef) -> crate::types::ActorRef {
    let kind = match a.kind.to_ascii_lowercase().as_str() {
        "service" => crate::types::ActorKind::Service,
        _ => crate::types::ActorKind::User,
    };
    crate::types::ActorRef {
        id: crate::types::ActorId(a.id.clone()),
        kind,
    }
}

fn types_actor_to_api(a: &crate::types::ActorRef) -> api::ActorRef {
    let kind = match a.kind {
        crate::types::ActorKind::User => "user",
        crate::types::ActorKind::Service => "service",
    };
    api::ActorRef {
        id: a.id.0.clone(),
        kind: kind.to_string(),
    }
}

fn gate_record_to_api(r: &crate::gate::GateRecord) -> api::Gate {
    let members = r
        .members
        .iter()
        .map(|m| api::GateMember {
            actor: Some(types_actor_to_api(&m.actor)),
            roles: m.roles.clone(),
            permissions: m.permissions.iter().map(permission_to_str).collect(),
        })
        .collect();
    api::Gate {
        id: r.id.0.clone(),
        name: r.base.name.clone(),
        owner: Some(types_actor_to_api(&r.owner)),
        members,
    }
}

#[allow(clippy::result_large_err)]
fn api_gate_to_record(g: api::Gate) -> Result<crate::gate::GateRecord, Status> {
    use crate::gate::GateRecord;
    let id = crate::types::GateId(g.id);
    let mut base = gate::Gate::default();
    if !g.name.is_empty() {
        base.name = g.name.clone();
    } else {
        base.name = id.0.clone();
    }
    let owner = if let Some(o) = g.owner.as_ref() {
        api_actor_to_types(o)
    } else {
        return Err(Status::invalid_argument("gate.owner is required"));
    };
    let mut rec = GateRecord::new(id, base, owner);
    // members
    for m in g.members.into_iter() {
        if let Some(ar) = m.actor.as_ref() {
            let actor = api_actor_to_types(ar);
            let perms = m
                .permissions
                .iter()
                .filter_map(|s| permission_from_str(s))
                .collect();
            let gm = crate::gate::GateMember {
                actor,
                roles: m.roles.clone(),
                permissions: perms,
            };
            rec.upsert_member(gm);
        }
    }
    Ok(rec)
}

// Normalize an incoming confirmation envelope which may be provided as
// raw JSON bytes or as Base64 (URL-safe preferred). Returns the decoded
// raw JSON bytes.
fn normalize_envelope_bytes(raw: &[u8]) -> Vec<u8> {
    // If it already looks like JSON, keep as-is to preserve exact bytes
    if let Some(b'{') = raw.first().copied() {
        return raw.to_vec();
    }
    // Try UTF-8 interpretation for trimming and base64 decoding
    if let Ok(s) = std::str::from_utf8(raw) {
        let trimmed = s.trim();
        if trimmed.starts_with('{') {
            return trimmed.as_bytes().to_vec();
        }
        // Try URL-safe without padding first, then URL-safe with padding, then standard
        if let Ok(decoded) =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(trimmed.as_bytes())
        {
            return decoded;
        }
        if let Ok(decoded) = base64::engine::general_purpose::URL_SAFE.decode(trimmed.as_bytes()) {
            return decoded;
        }
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(trimmed.as_bytes()) {
            return decoded;
        }
    }
    // Fallback to raw bytes when nothing matched
    raw.to_vec()
}

// Verify an ed25519 signature using raw 32-byte public key.
pub(crate) fn verify_ed25519(pubkey: &[u8], message: &[u8], signature: &[u8]) -> bool {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let pk_bytes: [u8; 32] = match pubkey.try_into() {
        Ok(arr) => arr,
        Err(_) => return false,
    };
    let vk = match VerifyingKey::from_bytes(&pk_bytes) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let sig = match Signature::from_slice(signature) {
        Ok(s) => s,
        Err(_) => return false,
    };
    vk.verify(message, &sig).is_ok()
}

const REGISTRATION_TTL_SECS: u64 = 4 * 60 * 60; // 4 hours

#[tonic::async_trait]
impl api::gate_service_server::GateService for GateServiceImpl {
    async fn get_gate(
        &self,
        request: Request<api::GetGateRequest>,
    ) -> Result<Response<api::GetGateResponse>, Status> {
        let req = request.into_inner();
        let Some(db) = &self.state.surreal else {
            return Err(Status::failed_precondition("database not available"));
        };
        if req.id.is_empty() {
            return Err(Status::invalid_argument("id is required"));
        }
        let id = crate::types::GateId(req.id);
        match sdb::get_gate(db, &id).await {
            Ok(Some(rec)) => {
                let gate = gate_record_to_api(&rec);
                Ok(Response::new(api::GetGateResponse { gate: Some(gate) }))
            }
            Ok(None) => Err(Status::not_found("gate not found")),
            Err(e) => {
                error!(id=%id.0, error=?e, "surreal get gate failed");
                Err(Status::internal("get gate failed"))
            }
        }
    }

    async fn create_gate(
        &self,
        request: Request<api::CreateGateRequest>,
    ) -> Result<Response<api::CreateGateResponse>, Status> {
        let req = request.into_inner();
        let Some(db) = &self.state.surreal else {
            return Err(Status::failed_precondition("database not available"));
        };
        let Some(g) = req.gate else {
            return Err(Status::invalid_argument("gate is required"));
        };
        if g.id.is_empty() {
            return Err(Status::invalid_argument("gate.id is required"));
        }
        let rec = api_gate_to_record(g)?;
        if let Err(e) = sdb::put_gate(db, &rec).await {
            error!(id=%rec.id.0, error=?e, "surreal upsert gate failed");
            return Err(Status::internal("create gate failed"));
        }
        let gate = gate_record_to_api(&rec);
        Ok(Response::new(api::CreateGateResponse { gate: Some(gate) }))
    }

    async fn list_gates(
        &self,
        _request: Request<api::ListGatesRequest>,
    ) -> Result<Response<api::ListGatesResponse>, Status> {
        let Some(db) = &self.state.surreal else {
            return Err(Status::failed_precondition("database not available"));
        };
        match sdb::list_gates(db).await {
            Ok(list) => {
                let gates = list.iter().map(gate_record_to_api).collect();
                Ok(Response::new(api::ListGatesResponse {
                    gates,
                    next_page_token: String::new(),
                }))
            }
            Err(e) => {
                error!(error=?e, "surreal list gates failed");
                Err(Status::internal("list gates failed"))
            }
        }
    }
}

// ---- Component type conversions between API and server models ----
fn stored_file_to_api(f: &crate::component::StoredFile) -> api::StoredFile {
    api::StoredFile {
        name: f.name.clone(),
        rel_path: f.rel_path.clone(),
    }
}

fn stored_file_from_api(f: &api::StoredFile) -> crate::component::StoredFile {
    crate::component::StoredFile {
        name: f.name.clone(),
        rel_path: f.rel_path.clone(),
    }
}

fn component_record_to_api(r: &crate::component::ComponentRecord) -> api::Component {
    let name = r.base.get_name().to_string();
    let files = api::ComponentFiles {
        patches: r.files.patches.iter().map(stored_file_to_api).collect(),
        licenses: r.files.licenses.iter().map(stored_file_to_api).collect(),
        scripts: r.files.scripts.iter().map(stored_file_to_api).collect(),
    };
    // Prefer returning the exact JSON that was uploaded (if we stored it in metadata).
    let base_json_from_meta = r
        .metadata
        .as_ref()
        .and_then(|m| m.get("base_json"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let base_json =
        base_json_from_meta.unwrap_or_else(|| serde_json::to_string(&r.base).unwrap_or_default());
    api::Component {
        id: r.id.0.clone(),
        name,
        files: Some(files),
        base_json,
    }
}

#[allow(clippy::result_large_err)]
fn api_component_to_record(c: api::Component) -> Result<crate::component::ComponentRecord, Status> {
    use crate::component::ComponentRecord;
    let id = crate::types::ComponentId(c.id);
    // Prefer provided base_json if present; otherwise, synthesize a minimal base from the name/id.
    let base = if !c.base_json.is_empty() {
        serde_json::from_str::<component::Component>(&c.base_json)
            .map_err(|_| Status::invalid_argument("invalid base_json for component"))?
    } else {
        let name = if !c.name.is_empty() {
            c.name.clone()
        } else {
            id.0.clone()
        };
        component::Component::new(name, None::<&std::path::Path>)
            .map_err(|_| Status::invalid_argument("invalid component name"))?
    };
    let mut rec = ComponentRecord::new(id, base);
    if let Some(files) = c.files.as_ref() {
        rec.files.patches = files.patches.iter().map(stored_file_from_api).collect();
        rec.files.licenses = files.licenses.iter().map(stored_file_from_api).collect();
        rec.files.scripts = files.scripts.iter().map(stored_file_from_api).collect();
    }
    // Stash the original base_json string in metadata for lossless round-trip and to avoid
    // serialization failures on the server side. This keeps storage schema unchanged.
    if !c.base_json.is_empty() {
        rec.metadata = Some(serde_json::json!({"base_json": c.base_json}));
    }
    Ok(rec)
}

#[tonic::async_trait]
impl api::component_service_server::ComponentService for ComponentServiceImpl {
    async fn get_component(
        &self,
        request: Request<api::GetComponentRequest>,
    ) -> Result<Response<api::GetComponentResponse>, Status> {
        let req = request.into_inner();
        let Some(db) = &self.state.surreal else {
            return Err(Status::failed_precondition("database not available"));
        };
        if req.id.is_empty() {
            return Err(Status::invalid_argument("id is required"));
        }
        let id = crate::types::ComponentId(req.id);
        match sdb::get_component(db, &id).await {
            Ok(Some(rec)) => {
                let comp = component_record_to_api(&rec);
                Ok(Response::new(api::GetComponentResponse {
                    component: Some(comp),
                }))
            }
            Ok(None) => Err(Status::not_found("component not found")),
            Err(e) => {
                error!(id=%id.0, error=?e, "surreal get component failed");
                Err(Status::internal("get component failed"))
            }
        }
    }

    async fn create_component(
        &self,
        request: Request<api::CreateComponentRequest>,
    ) -> Result<Response<api::CreateComponentResponse>, Status> {
        let req = request.into_inner();
        let Some(db) = &self.state.surreal else {
            return Err(Status::failed_precondition("database not available"));
        };
        let Some(c) = req.component else {
            return Err(Status::invalid_argument("component is required"));
        };
        if c.id.is_empty() {
            return Err(Status::invalid_argument("component.id is required"));
        }
        let rec = api_component_to_record(c)?;
        if let Err(e) = sdb::put_component(db, &rec).await {
            error!(id=%rec.id.0, error=?e, "surreal upsert component failed");
            return Err(Status::internal("create component failed"));
        }
        let comp = component_record_to_api(&rec);
        Ok(Response::new(api::CreateComponentResponse {
            component: Some(comp),
        }))
    }

    async fn list_components(
        &self,
        _request: Request<api::ListComponentsRequest>,
    ) -> Result<Response<api::ListComponentsResponse>, Status> {
        let Some(db) = &self.state.surreal else {
            return Err(Status::failed_precondition("database not available"));
        };
        match sdb::list_components(db).await {
            Ok(list) => {
                let components = list.iter().map(component_record_to_api).collect();
                Ok(Response::new(api::ListComponentsResponse {
                    components,
                    next_page_token: String::new(),
                }))
            }
            Err(e) => {
                error!(error=?e, "surreal list components failed");
                Err(Status::internal("list components failed"))
            }
        }
    }
}

#[tonic::async_trait]
impl api::auth_service_server::AuthService for AuthServiceImpl {
    async fn issue_token(
        &self,
        request: Request<api::IssueTokenRequest>,
    ) -> Result<Response<api::IssueTokenResponse>, Status> {
        #[derive(Serialize, Deserialize)]
        struct Claims {
            sub: String,
            actor_kind: String,
            roles: Vec<String>,
            permissions: Vec<String>,
            exp: usize,
            iat: usize,
            iss: String,
            jti: String,
        }
        let req = request.into_inner();
        if req.subject.is_empty() {
            return Err(Status::invalid_argument("subject is required"));
        }
        // Default TTL to 1 hour if not provided
        let ttl = if req.ttl_seconds == 0 {
            3600
        } else {
            req.ttl_seconds
        } as u64;
        let now = now_sec() as usize;
        let exp = (now_sec() + ttl) as usize;
        let actor_kind = match api::ActorKind::try_from(req.actor_kind) {
            Ok(k) => format!("{:?}", k),
            Err(_) => return Err(Status::invalid_argument("invalid actor_kind")),
        };
        let claims = Claims {
            sub: req.subject.clone(),
            actor_kind,
            roles: req.roles.clone(),
            permissions: req.permissions.clone(),
            exp,
            iat: now,
            iss: "forged".to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
        };
        let secret = self.state.server_private_ssh.as_bytes();
        let token = match jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret),
        ) {
            Ok(t) => t,
            Err(e) => {
                error!(error=?e, "jwt encode failed");
                return Err(Status::internal("token generation failed"));
            }
        };
        let refresh = uuid::Uuid::new_v4().to_string();
        Ok(Response::new(api::IssueTokenResponse {
            access_token: token,
            refresh_token: refresh,
            expires_at: exp as u64,
        }))
    }

    async fn register_actor(
        &self,
        request: Request<api::RegisterActorRequest>,
    ) -> Result<Response<api::RegisterActorResponse>, Status> {
        let req = request.into_inner();
        let actor_id = req.actor_id.clone();
        if actor_id.is_empty() {
            return Err(Status::invalid_argument("actor_id is required"));
        }
        let email = req.email.clone();
        if email.is_empty() {
            return Err(Status::invalid_argument("email is required"));
        }
        let Ok(actor_kind) = api::ActorKind::try_from(req.actor_kind) else {
            return Err(Status::invalid_argument("invalid actor_kind"));
        };
        let public_key = req.public_key.clone();
        if public_key.is_none() {
            return Err(Status::invalid_argument("public_key is required"));
        }
        let pk = public_key.as_ref().unwrap();
        if pk.algorithm.is_empty() {
            return Err(Status::invalid_argument("public_key.algorithm is required"));
        }
        // Build envelope
        let issued_at = now_sec();
        let expires_at = issued_at + REGISTRATION_TTL_SECS;
        let key_id = if pk.key_id.is_empty() {
            "initial".to_string()
        } else {
            pk.key_id.clone()
        };
        let meta = ConfirmationMeta {
            actor_id: actor_id.clone(),
            actor_kind: format!("{:?}", actor_kind),
            issued_at,
            expires_at,
            key_id: key_id.clone(),
        };
        let payload = ConfirmationPayload {
            nonce: uuid::Uuid::new_v4().to_string(),
            server_key_id: "server-default".to_string(),
        };
        let payload_json =
            serde_json::to_string(&payload).map_err(|_| Status::internal("serialize payload"))?;
        // Encrypt payload JSON using age with the server's SSH public key
        let recipient = age::ssh::Recipient::from_str(&self.state.server_public_ssh)
            .map_err(|_| Status::internal("invalid server public ssh key"))?;
        let mut ciphertext_buf = vec![];
        {
            let encryptor =
                Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                    .map_err(|_| Status::internal("encrypt setup"))?;
            let mut writer = encryptor
                .wrap_output(&mut ciphertext_buf)
                .map_err(|_| Status::internal("encrypt wrap output"))?;
            writer
                .write_all(payload_json.as_bytes())
                .map_err(|_| Status::internal("encrypt write"))?;
            writer
                .finish()
                .map_err(|_| Status::internal("encrypt finish"))?;
        }
        let ciphertext_b64 = base64::engine::general_purpose::STANDARD.encode(&ciphertext_buf);
        let envelope = ConfirmationEnvelope {
            meta,
            payload_ciphertext_b64: ciphertext_b64,
            encrypted_for_algorithm: pk.algorithm.clone(),
        };
        let envelope_bytes =
            serde_json::to_vec(&envelope).map_err(|_| Status::internal("serialize envelope"))?;

        // Attempt to encrypt the envelope for the actor using their SSH public key (best-effort)
        // If successful, we include the Base64-URL (no padding) ciphertext in the email for
        // users to decrypt locally with age/rage CLI.
        let mut actor_encrypted_b64: Option<String> = None;
        if let Some(pk) = &public_key {
            if let Ok(actor_pub_str) = std::str::from_utf8(&pk.public_key) {
                if let Ok(actor_recipient) = age::ssh::Recipient::from_str(actor_pub_str) {
                    let _ = (|| {
                        let mut sink = Vec::new();
                        let encryptor = Encryptor::with_recipients(std::iter::once(
                            &actor_recipient as &dyn age::Recipient,
                        ))
                        .map_err(|_| ())?;
                        let mut writer = encryptor.wrap_output(&mut sink).map_err(|_| ())?;
                        writer.write_all(&envelope_bytes).map_err(|_| ())?;
                        writer.finish().map_err(|_| ())?;
                        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&sink);
                        actor_encrypted_b64 = Some(b64);
                        info!(
                            cipher_len = sink.len(),
                            "envelope encrypted for actor using age/ssh"
                        );
                        Ok::<(), ()>(())
                    })();
                } else {
                    warn!(
                        "actor public key not parseable as openssh; skipping envelope encryption"
                    );
                }
            } else {
                warn!("actor public key not utf8; skipping envelope encryption");
            }
        }

        // Store pending (Surreal if configured, otherwise in-memory)
        let k = actor_key(&actor_id, actor_kind);
        if let Some(db) = &self.state.surreal {
            let rec = sdb::PendingRegistrationRec {
                id: k.clone(),
                actor_id: actor_id.clone(),
                actor_kind: actor_kind as i32,
                expires_at,
                envelope: envelope_bytes.clone(),
            };
            if let Err(e) = sdb::upsert_pending_registration(db, &rec).await {
                error!(actor_id=%actor_id, error=?e, "pending upsert failed");
                return Err(Status::internal("pending upsert failed"));
            }
        } else {
            let pending = PendingRegistration {
                id: k.clone(),
                actor_id: actor_id.clone(),
                actor_kind,
                public_key: public_key.clone(),
                expires_at,
                envelope: envelope_bytes.clone(),
            };
            let mut guard = self
                .state
                .pending
                .lock()
                .map_err(|_| Status::internal("pending lock"))?;
            guard.insert(k, pending);
        }

        // Send email if SMTP configured; otherwise just log
        if let (Some(mailer), Some(from_addr)) = (&self.state.mailer, &self.state.mail_from) {
            // Validate recipient address (email)
            let to_mb = email
                .parse::<Mailbox>()
                .map_err(|_| Status::invalid_argument("email is not a valid email address"))?;

            // Validate configured from address
            let from_mb = from_addr
                .parse::<Mailbox>()
                .map_err(|_| Status::invalid_argument("invalid SMTP from address configured"))?;

            // Build human-readable body with instructions and include the envelope
            // Encode the envelope using URL-safe Base64 without padding for easy copy/paste and CLI usage
            let envelope_text =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&envelope_bytes);
            let actor_section = if let Some(cipher) = actor_encrypted_b64.as_deref() {
                format!(
                    concat!(
                        "\n\n",
                        "For your convenience, the envelope is also encrypted to your SSH public key using age.\n",
                        "You can decrypt it locally with:\n",
                        "  echo '{{cipher}}' | base64 -d | rage -d -i ~/.ssh/id_ed25519 > envelope.json\n",
                        "or using age (if installed) similarly.\n\n",
                        "--- BEGIN AGE-ENCRYPTED ENVELOPE (Base64-URL, no padding) ---\n",
                        "{cipher}\n",
                        "--- END AGE-ENCRYPTED ENVELOPE ---\n"
                    ),
                    cipher = cipher
                )
            } else {
                String::new()
            };

            let body = format!(
                concat!(
                    "Hello,\n\n",
                    "You recently requested to register an actor in Forge.\n",
                    "To complete your registration, copy the envelope below exactly as-is and send it back to the server ",
                    "using your client via the RegistrationConfirmation request.\n",
                    "The envelope below is Base64-URL encoded (no padding).\n",
                    "Do not share this envelope with anyone. It expires at UNIX time: {expires}.\n\n",
                    "--- BEGIN REGISTRATION ENVELOPE ---\n",
                    "{envelope}\n",
                    "--- END REGISTRATION ENVELOPE ---\n",
                    "{actor_section}",
                    "\nIf you did not initiate this request, you can ignore this email."
                ),
                expires = expires_at,
                envelope = envelope_text,
                actor_section = actor_section
            );

            // Build the message
            let email = Message::builder()
                .from(from_mb)
                .to(to_mb)
                .subject("Forge registration confirmation")
                .header(lettre::message::header::ContentType::TEXT_PLAIN)
                .body(body)
                .map_err(|_| Status::internal("failed to build registration email"))?;

            // Send the message
            match mailer.send(email).await {
                Ok(_) => info!(actor_id=%actor_id, "registration email sent"),
                Err(e) => {
                    warn!(error=?e, "failed to send registration email");
                    return Err(Status::internal("failed to send registration email"));
                }
            }
        } else {
            info!(actor_id=%actor_id, algorithm=%pk.algorithm, "registration envelope generated (no SMTP configured)");
        }

        Ok(Response::new(api::RegisterActorResponse { created: true }))
    }

    async fn registration_confirmation(
        &self,
        request: Request<api::RegistrationConfirmationRequest>,
    ) -> Result<Response<api::RegistrationConfirmationResponse>, Status> {
        let req = request.into_inner();
        if req.actor_id.is_empty() {
            return Err(Status::invalid_argument("actor_id is required"));
        }
        let Ok(actor_kind) = api::ActorKind::try_from(req.actor_kind) else {
            return Err(Status::invalid_argument("invalid actor_kind"));
        };
        let k = actor_key(&req.actor_id, actor_kind);
        let now = now_sec();
        if let Some(db) = &self.state.surreal {
            // Surreal-backed pending
            let p = sdb::get_pending_registration(db, &k)
                .await
                .map_err(|e| {
                    error!(actor_id=%req.actor_id, error=?e, "surreal get pending failed");
                    Status::internal("surreal get pending")
                })?
                .ok_or_else(|| Status::failed_precondition("no pending registration for actor"))?;
            if p.expires_at <= now {
                let _ = sdb::delete_pending_registration(db, &k).await;
                return Err(Status::failed_precondition("pending registration expired"));
            }
            // Normalize incoming envelope (supports Base64-URL and raw JSON)
            let provided = normalize_envelope_bytes(&req.confirmation_envelope);
            if p.envelope != provided {
                return Err(Status::invalid_argument(
                    "confirmation envelope does not match",
                ));
            }
            let env: ConfirmationEnvelope = serde_json::from_slice(&provided)
                .map_err(|_| Status::invalid_argument("invalid envelope json"))?;
            if env.meta.actor_id != req.actor_id {
                return Err(Status::invalid_argument("envelope actor_id mismatch"));
            }
            if env.meta.expires_at <= now {
                let _ = sdb::delete_pending_registration(db, &k).await;
                return Err(Status::failed_precondition("envelope expired"));
            }
            // Decrypt inner payload using server SSH private key via age
            let cipher = base64::engine::general_purpose::STANDARD
                .decode(env.payload_ciphertext_b64.as_bytes())
                .map_err(|_| Status::invalid_argument("invalid payload ciphertext"))?;
            let decryptor = Decryptor::new(Cursor::new(cipher))
                .map_err(|_| Status::invalid_argument("invalid age payload"))?;
            let reader = BufReader::new(Cursor::new(
                self.state.server_private_ssh.clone().into_bytes(),
            ));
            let identity = age::ssh::Identity::from_buffer(reader, None)
                .map_err(|_| Status::internal("invalid server private ssh key"))?;
            let mut reader = decryptor
                .decrypt(std::iter::once(&identity as &dyn age::Identity))
                .map_err(|_| Status::invalid_argument("decryption failed"))?;
            let mut plaintext = Vec::new();
            reader
                .read_to_end(&mut plaintext)
                .map_err(|_| Status::internal("decrypt read"))?;
            let _ = sdb::delete_pending_registration(db, &k).await; // remove pending after success
        } else {
            // In-memory pending
            let mut pending_guard = self
                .state
                .pending
                .lock()
                .map_err(|_| Status::internal("pending lock"))?;
            let Some(p) = pending_guard.get(&k) else {
                return Err(Status::failed_precondition(
                    "no pending registration for actor",
                ));
            };
            if p.expires_at <= now {
                // expire and remove
                pending_guard.remove(&k);
                return Err(Status::failed_precondition("pending registration expired"));
            }
            // Validate envelope matches what we sent (normalize first to support Base64-URL input)
            let provided = normalize_envelope_bytes(&req.confirmation_envelope);
            if p.envelope != provided {
                return Err(Status::invalid_argument(
                    "confirmation envelope does not match",
                ));
            }
            // Parse envelope and check metadata alignment
            let env: ConfirmationEnvelope = serde_json::from_slice(&provided)
                .map_err(|_| Status::invalid_argument("invalid envelope json"))?;
            if env.meta.actor_id != req.actor_id {
                return Err(Status::invalid_argument("envelope actor_id mismatch"));
            }
            if env.meta.expires_at <= now {
                pending_guard.remove(&k);
                return Err(Status::failed_precondition("envelope expired"));
            }
            // Decrypt inner payload using server SSH private key via age
            let cipher = base64::engine::general_purpose::STANDARD
                .decode(env.payload_ciphertext_b64.as_bytes())
                .map_err(|_| Status::invalid_argument("invalid payload ciphertext"))?;
            let decryptor = Decryptor::new(Cursor::new(cipher))
                .map_err(|_| Status::invalid_argument("invalid age payload"))?;
            let reader = BufReader::new(Cursor::new(
                self.state.server_private_ssh.clone().into_bytes(),
            ));
            let identity = age::ssh::Identity::from_buffer(reader, None)
                .map_err(|_| Status::internal("invalid server private ssh key"))?;
            let mut reader = decryptor
                .decrypt(std::iter::once(&identity as &dyn age::Identity))
                .map_err(|_| Status::invalid_argument("decryption failed"))?;
            let mut plaintext = Vec::new();
            reader
                .read_to_end(&mut plaintext)
                .map_err(|_| Status::internal("decrypt read"))?;
            // Remove pending entry
            pending_guard.remove(&k);
            drop(pending_guard);
        }

        Ok(Response::new(api::RegistrationConfirmationResponse {
            confirmed: true,
        }))
    }

    async fn add_actor_key(
        &self,
        request: Request<api::AddActorKeyRequest>,
    ) -> Result<Response<api::AddActorKeyResponse>, Status> {
        let req = request.into_inner();
        if req.actor_id.is_empty() {
            return Err(Status::invalid_argument("actor_id is required"));
        }
        let _kind = api::ActorKind::try_from(req.actor_kind)
            .map_err(|_| Status::invalid_argument("invalid actor_kind"))?;
        let Some(pk) = req.public_key else {
            return Err(Status::invalid_argument("public_key is required"));
        };
        if pk.algorithm.is_empty() {
            return Err(Status::invalid_argument("public_key.algorithm is required"));
        }
        if pk.public_key.is_empty() {
            return Err(Status::invalid_argument(
                "public_key.public_key is required",
            ));
        }
        // Persist in SurrealDB if available
        if let Some(db) = &self.state.surreal {
            let rec = sdb::ActorKeyRec {
                id: String::new(),
                actor_id: req.actor_id.clone(),
                key_id: pk.key_id.clone(),
                algorithm: pk.algorithm.clone(),
                public_key: pk.public_key.clone(),
            };
            if let Err(e) = sdb::upsert_actor_key(db, &rec).await {
                error!(actor_id=%req.actor_id, key_id=%pk.key_id, error=?e, "persist actor key failed");
                return Err(Status::internal("persist actor key failed"));
            }
            info!(actor_id=%req.actor_id, key_id=%pk.key_id, algorithm=%pk.algorithm, "actor key persisted");
        } else {
            info!(actor_id=%req.actor_id, key_id=%pk.key_id, algorithm=%pk.algorithm, "AddActorKey accepted (no DB configured)");
        }
        Ok(Response::new(api::AddActorKeyResponse { added: true }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::forged::api::v1::auth_service_server::AuthService;
    use crate::api::forged::api::v1::{
        auth_service_server::AuthServiceServer, component_service_server::ComponentServiceServer,
        gate_service_server::GateServiceServer,
    };
    use tonic::Request;

    #[test]
    fn servers_construct() {
        let _g = GateServiceServer::new(GateServiceImpl::default());
        let _c = ComponentServiceServer::new(ComponentServiceImpl::default());
        let _a = AuthServiceServer::new(AuthServiceImpl::default());
    }

    #[tokio::test]
    async fn registration_flow_success() {
        let svc = AuthServiceImpl::default();
        let actor_id = "alice@forge.local".to_string();
        let actor_kind = api::ActorKind::User; // note: this is the enum type, prost::Enumeration; as i32 when placed in messages
        let req = api::RegisterActorRequest {
            actor_id: actor_id.clone(),
            actor_kind: actor_kind as i32,
            public_key: Some(api::PublicKey {
                key_id: "k1".into(),
                algorithm: "ed25519".into(),
                public_key: vec![1, 2, 3],
            }),
            proof: None,
            email: "alice@example.com".to_string(),
        };
        let _ = svc
            .register_actor(Request::new(req))
            .await
            .expect("register ok");

        // Grab the envelope from pending store
        let k = super::actor_key(&actor_id, actor_kind);
        let pending_map = svc.state.pending.lock().unwrap();
        let p = pending_map.get(&k).expect("pending exists");
        let env = p.envelope.clone();
        drop(pending_map);

        let confirm_req = api::RegistrationConfirmationRequest {
            actor_id: actor_id.clone(),
            actor_kind: actor_kind as i32,
            confirmation_envelope: env,
        };
        let resp = svc
            .registration_confirmation(Request::new(confirm_req))
            .await
            .expect("confirm ok");
        assert!(resp.into_inner().confirmed);
    }

    #[tokio::test]
    async fn registration_flow_expired() {
        let svc = AuthServiceImpl::default();
        let actor_id = "bob@forge.local".to_string();
        let actor_kind = api::ActorKind::User;
        let req = api::RegisterActorRequest {
            actor_id: actor_id.clone(),
            actor_kind: actor_kind as i32,
            public_key: Some(api::PublicKey {
                key_id: "k2".into(),
                algorithm: "ed25519".into(),
                public_key: vec![4, 5, 6],
            }),
            proof: None,
            email: "bob@example.com".to_string(),
        };
        let _ = svc
            .register_actor(Request::new(req))
            .await
            .expect("register ok");
        let k = super::actor_key(&actor_id, actor_kind);
        {
            let mut pending_map = svc.state.pending.lock().unwrap();
            let p = pending_map.get_mut(&k).unwrap();
            p.expires_at = super::now_sec() - 1;
        }
        let env = {
            let pending_map = svc.state.pending.lock().unwrap();
            pending_map.get(&k).unwrap().envelope.clone()
        };
        let confirm_req = api::RegistrationConfirmationRequest {
            actor_id: actor_id.clone(),
            actor_kind: actor_kind as i32,
            confirmation_envelope: env,
        };
        let err = svc
            .registration_confirmation(Request::new(confirm_req))
            .await
            .err()
            .expect("should fail");
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    }
}
