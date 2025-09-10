use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

// Use client types generated from proto in this crate (see build.rs)
use crate::api::forged::api::v1 as api;
use tonic::transport::Channel;
use tonic::Request;

#[derive(
    Debug, Clone, Copy, clap::ValueEnum, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd,
)]
pub enum ActorKind {
    User,
    Service,
}

impl From<ActorKind> for api::ActorKind {
    fn from(value: ActorKind) -> Self {
        match value {
            ActorKind::User => api::ActorKind::User,
            ActorKind::Service => api::ActorKind::Service,
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
#[diagnostic(code(ips::auth_error), help("check server address and parameters"))]
pub enum AuthClientError {
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),

    #[error(transparent)]
    Status(#[from] tonic::Status),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("failed to read file at {0}")]
    ReadFile(String),

    #[error("invalid server url: {0}")]
    InvalidServerUrl(String),
}

pub type Result<T, E = AuthClientError> = miette::Result<T, E>;

#[derive(Clone)]
pub struct AuthClient {
    server: String,
    channel: Channel,
}

impl AuthClient {
    pub async fn connect<S: Into<String>>(server: S) -> Result<Self> {
        let server = server.into();
        // tonic expects http/https scheme
        if !server.starts_with("http://") && !server.starts_with("https://") {
            return Err(AuthClientError::InvalidServerUrl(server).into());
        }
        let endpoint = Channel::from_shared(server.clone())
            .map_err(|_| AuthClientError::InvalidServerUrl(server.clone()))?;
        let channel = endpoint.connect().await?;
        Ok(Self { server, channel })
    }

    fn client(&self) -> api::auth_service_client::AuthServiceClient<Channel> {
        api::auth_service_client::AuthServiceClient::new(self.channel.clone())
    }

    pub async fn register_actor(
        &self,
        actor_id: String,
        kind: ActorKind,
        public_key_path: &Path,
        algorithm_hint: Option<String>,
    ) -> Result<()> {
        // Read SSH public key file (OpenSSH format recommended)
        let mut file = fs::File::open(public_key_path).await.map_err(|e| {
            info!(path=%public_key_path.display(), error=?e, "failed to open public key");
            e
        })?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(|e| {
            info!(path=%public_key_path.display(), error=?e, "failed to read public key");
            e
        })?;

        let algorithm = algorithm_hint.unwrap_or_else(|| guess_ssh_algorithm(&buf));
        debug!(algorithm=%algorithm, bytes=%buf.len(), "using algorithm for public key");

        let pk = api::PublicKey {
            key_id: "initial".to_string(),
            algorithm,
            public_key: buf,
        };
        // Minimal proof placeholder (server-side currently not verifying)
        let proof = api::SignedMessage {
            algorithm: String::new(),
            message: vec![],
            signature: vec![],
            key_id: String::new(),
        };

        let req = api::RegisterActorRequest {
            actor_id,
            actor_kind: api::ActorKind::from(kind) as i32,
            public_key: Some(pk),
            proof: Some(proof),
        };

        let mut client = self.client();
        let _resp = client.register_actor(Request::new(req)).await?;
        Ok(())
    }

    pub async fn confirm_registration(
        &self,
        actor_id: String,
        kind: ActorKind,
        envelope_path: &Path,
    ) -> Result<()> {
        let envelope_bytes = fs::read(envelope_path).await.map_err(|e| {
            info!(path=%envelope_path.display(), error=?e, "failed to read envelope");
            e
        })?;
        let req = api::RegistrationConfirmationRequest {
            actor_id,
            actor_kind: api::ActorKind::from(kind) as i32,
            confirmation_envelope: envelope_bytes,
        };
        let mut client = self.client();
        let _resp = client.registration_confirmation(Request::new(req)).await?;
        Ok(())
    }
}

fn guess_ssh_algorithm(key_bytes: &[u8]) -> String {
    // Heuristic: look at OpenSSH header prefix
    let s = std::str::from_utf8(key_bytes).unwrap_or("");
    if s.contains("ssh-ed25519") {
        "ed25519".to_string()
    } else if s.contains("ecdsa-sha2-nistp256") {
        "ecdsa-p256".to_string()
    } else if s.contains("ecdsa-sha2-nistp384") {
        "ecdsa-p384".to_string()
    } else if s.contains("ecdsa-sha2-nistp521") {
        "ecdsa-p521".to_string()
    } else if s.contains("ssh-rsa") {
        // Prefer rsa-pss naming if server expects; keep generic
        "rsa-ssh".to_string()
    } else {
        // Fallback – server will validate
        "unknown".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LoginEntry {
    pub actor_id: String,
    pub kind: ActorKind,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AuthState {
    // hostname -> set of (actor_id, kind)
    pub logins: BTreeMap<String, BTreeSet<LoginEntryKey>>, // internal uniq key
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct LoginEntryKey {
    pub actor_id: String,
    pub kind: ActorKind,
}

impl From<&LoginEntry> for LoginEntryKey {
    fn from(e: &LoginEntry) -> Self {
        LoginEntryKey {
            actor_id: e.actor_id.clone(),
            kind: e.kind,
        }
    }
}

impl AuthState {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path)?;
        let state = serde_json::from_slice(&bytes).unwrap_or_default();
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let buf = serde_json::to_vec_pretty(self).expect("serialize state");
        std::fs::write(path, buf)
    }

    pub fn add_login(&mut self, host: &str, entry: LoginEntry) {
        let set = self.logins.entry(host.to_string()).or_default();
        set.insert(LoginEntryKey::from(&entry));
    }

    pub fn list_for(&self, host: &str) -> Vec<LoginEntry> {
        self.logins
            .get(host)
            .map(|s| {
                s.iter()
                    .map(|k| LoginEntry {
                        actor_id: k.actor_id.clone(),
                        kind: k.kind,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub fn default_auth_state_path() -> PathBuf {
    // Use directories crate via crate::get_project_dir
    if let Ok(pd) = crate::get_project_dir() {
        pd.config_dir().join("auth_state.json")
    } else {
        PathBuf::from("auth_state.json")
    }
}

pub fn server_url_from_host(host: &str) -> String {
    // Accept host or host:port and default scheme http
    format!("http://{}", host)
}
