use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use lettre::AsyncTransport;
use lettre::{message::Mailbox, AsyncSmtpTransport, Message, Tokio1Executor};
// SurrealDB storage
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
    ) -> Self {
        let mut state = State::default();
        state.surreal = Some(db);
        state.server_private_ssh = private_ssh;
        state.server_public_ssh = public_ssh;
        state.mailer = mailer;
        state.mail_from = mail_from;
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

impl AuthServiceImpl {
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
        let mut state = State::default();
        state.surreal = Some(db);
        state.server_private_ssh = private_ssh;
        state.server_public_ssh = public_ssh;
        state.mailer = mailer;
        state.mail_from = mail_from;
        Self {
            state: Arc::new(state),
        }
    }

    pub fn with_keys(private_ssh: String, public_ssh: String) -> Self {
        let mut state = State::default();
        state.server_private_ssh = private_ssh;
        state.server_public_ssh = public_ssh;
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
        let mut state = State::default();
        state.server_private_ssh = private_ssh;
        state.server_public_ssh = public_ssh;
        state.mailer = mailer;
        state.mail_from = mail_from;
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
        _request: Request<api::IssueTokenRequest>,
    ) -> Result<Response<api::IssueTokenResponse>, Status> {
        Err(Status::unimplemented("IssueToken not implemented"))
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
        let Some(actor_kind) = api::ActorKind::from_i32(req.actor_kind) else {
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
            // Validate recipient address (actor_id)
            let to_mb = actor_id
                .parse::<Mailbox>()
                .map_err(|_| Status::invalid_argument("actor_id is not a valid email address"))?;

            // Validate configured from address
            let from_mb = from_addr
                .parse::<Mailbox>()
                .map_err(|_| Status::invalid_argument("invalid SMTP from address configured"))?;

            // Build human-readable body with instructions and include the envelope
            // Encode the envelope using URL-safe Base64 without padding for easy copy/paste and CLI usage
            let envelope_text =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&envelope_bytes);
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
                    "--- END REGISTRATION ENVELOPE ---\n\n",
                    "If you did not initiate this request, you can ignore this email."
                ),
                expires = expires_at,
                envelope = envelope_text
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
        let Some(actor_kind) = api::ActorKind::from_i32(req.actor_kind) else {
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
        _request: Request<api::AddActorKeyRequest>,
    ) -> Result<Response<api::AddActorKeyResponse>, Status> {
        Err(Status::unimplemented("AddActorKey not implemented"))
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
        let actor_id = "user@example.com".to_string();
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
        let actor_id = "user2@example.com".to_string();
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
