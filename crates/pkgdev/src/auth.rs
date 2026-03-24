use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use base64::Engine;
use chrono::{DateTime, Utc};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, info, warn};

// Use client types generated from proto in this crate (see build.rs)
use crate::api::forged::api::v1 as api;
use crate::api::forged::api::v2 as api_v2;
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
    #[allow(dead_code)]
    server: String,
    channel: Channel,
}

impl AuthClient {
    pub async fn connect<S: Into<String>>(server: S) -> Result<Self> {
        let server = server.into();
        // tonic expects http/https scheme
        if !server.starts_with("http://") && !server.starts_with("https://") {
            return Err(AuthClientError::InvalidServerUrl(server));
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
        email: String,
        kind: ActorKind,
        public_key_path: &Path,
        algorithm_hint: Option<String>,
        token: &str,
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
            email,
        };

        let mut client = self.client();
        let _resp = client
            .register_actor(authenticated_request(req, token))
            .await?;
        Ok(())
    }

    pub async fn confirm_registration(
        &self,
        actor_id: String,
        kind: ActorKind,
        envelope: &str,
        token: &str,
    ) -> Result<()> {
        // The envelope is provided directly (Base64-URL or raw JSON). Send as-is.
        let envelope_bytes = envelope.as_bytes().to_vec();
        let req = api::RegistrationConfirmationRequest {
            actor_id,
            actor_kind: api::ActorKind::from(kind) as i32,
            confirmation_envelope: envelope_bytes,
        };
        let mut client = self.client();
        let _resp = client
            .registration_confirmation(authenticated_request(req, token))
            .await?;
        Ok(())
    }

    /// Confirm registration using an age-encrypted envelope addressed to the actor's SSH key.
    /// - `encrypted_b64`: Base64-URL (no padding preferred) encoded ciphertext from the email
    /// - `identity_path`: path to your SSH private key (e.g., ~/.ssh/id_ed25519)
    pub async fn confirm_registration_encrypted(
        &self,
        actor_id: String,
        kind: ActorKind,
        encrypted_b64: &str,
        identity_path: &Path,
        token: &str,
    ) -> Result<()> {
        use age::Decryptor;
        use std::io::{BufReader, Cursor, Read};
        // Decode Base64 (try URL-safe no pad, then URL-safe, then standard)
        let cipher = decode_b64_any(encrypted_b64.as_bytes());
        // Load SSH identity (unencrypted private key)
        let ident_bytes = std::fs::read(identity_path).map_err(|e| {
            info!(path=%identity_path.display(), error=?e, "failed to read identity");
            e
        })?;
        let reader = BufReader::new(Cursor::new(ident_bytes));
        let identity = age::ssh::Identity::from_buffer(reader, None)
            .map_err(|e| {
                info!(path=%identity_path.display(), error=?e, "invalid SSH identity (is it encrypted?)");
                std::io::Error::other("invalid SSH identity")
            })?;
        let decryptor = Decryptor::new(Cursor::new(cipher)).map_err(|e| {
            info!(error=?e, "invalid age ciphertext for envelope");
            std::io::Error::other("invalid age ciphertext")
        })?;
        let mut r = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| {
                info!(error=?e, "failed to decrypt envelope with provided identity");
                std::io::Error::other("decrypt failed")
            })?;
        let mut envelope_bytes = Vec::new();
        r.read_to_end(&mut envelope_bytes)?;

        // Send the decrypted raw JSON envelope
        let req = api::RegistrationConfirmationRequest {
            actor_id,
            actor_kind: api::ActorKind::from(kind) as i32,
            confirmation_envelope: envelope_bytes,
        };
        let mut client = self.client();
        let _resp = client
            .registration_confirmation(authenticated_request(req, token))
            .await?;
        Ok(())
    }
}

fn decode_b64_any(input: &[u8]) -> Vec<u8> {
    if let Ok(d) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(input) {
        return d;
    }
    if let Ok(d) = base64::engine::general_purpose::URL_SAFE.decode(input) {
        return d;
    }
    if let Ok(d) = base64::engine::general_purpose::STANDARD.decode(input) {
        return d;
    }
    // Fallback: treat as UTF-8, trim and retry
    if let Ok(s) = std::str::from_utf8(input) {
        let t = s.trim();
        if let Ok(d) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(t.as_bytes()) {
            return d;
        }
        if let Ok(d) = base64::engine::general_purpose::URL_SAFE.decode(t.as_bytes()) {
            return d;
        }
        if let Ok(d) = base64::engine::general_purpose::STANDARD.decode(t.as_bytes()) {
            return d;
        }
    }
    input.to_vec()
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
    /// Currently selected context (host + login) for defaulting forge commands
    #[serde(default)]
    pub selected: Option<SelectedContext>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SelectedContext {
    pub host: String,
    pub actor_id: String,
    pub kind: ActorKind,
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

    pub fn set_selected<S: Into<String>>(&mut self, host: S, actor_id: S, kind: ActorKind) {
        self.selected = Some(SelectedContext {
            host: host.into(),
            actor_id: actor_id.into(),
            kind,
        });
    }

    pub fn clear_selected(&mut self) {
        self.selected = None;
    }

    pub fn get_selected(&self) -> Option<&SelectedContext> {
        self.selected.as_ref()
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
    // If the host already has a scheme, use it as-is
    if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("http://{}", host)
    }
}

/// Connect to a gRPC endpoint with improved error diagnostics for TLS failures.
///
/// If `tls_insecure` is true and the URL uses https://, TLS certificate
/// verification is skipped (useful for testing against staging certificates).
pub async fn connect_grpc(url: &str, tls_insecure: bool) -> miette::Result<Channel> {
    let is_tls = url.starts_with("https://");
    let mut endpoint = Channel::from_shared(url.to_string())
        .map_err(|_| miette::miette!("invalid gRPC endpoint: {}", url))?;

    if is_tls && tls_insecure {
        // tonic 0.11 doesn't expose danger_accept_invalid_certs, so we
        // downgrade to plaintext HTTP/2 when --tls-insecure is set.
        // The server must accept h2c (plaintext HTTP/2) on the same port.
        info!("TLS verification disabled (--tls-insecure), connecting without TLS");
        let insecure_url = url.replacen("https://", "http://", 1);
        endpoint = Channel::from_shared(insecure_url.clone())
            .map_err(|_| miette::miette!("invalid gRPC endpoint: {}", insecure_url))?;
    }

    match endpoint.connect().await {
        Ok(channel) => Ok(channel),
        Err(e) => {
            let err_str = format!("{}", e);
            let details = format!("{:?}", e);

            if is_tls
                && (details.contains("certificate")
                    || details.contains("ssl")
                    || details.contains("tls")
                    || details.contains("h2 protocol error")
                    || details.contains("connection error"))
            {
                Err(miette::miette!(
                    help = "The server's TLS certificate may not be trusted by this client.\n\
                            Try --tls-insecure for testing with staging certificates.\n\
                            If the server has no valid certificate yet, use http://<host>:<port>",
                    "TLS connection to {} failed: {}",
                    url,
                    err_str
                ))
            } else {
                Err(miette::miette!(
                    help = "Check that the server is running and the address is correct.\n\
                            Use http:// for plaintext or https:// for TLS connections.",
                    "failed to connect to forge at {}: {}",
                    url,
                    err_str
                ))
            }
        }
    }
}

/// Diagnose a gRPC call error with actionable help for common failures.
pub fn diagnose_rpc_error(url: &str, rpc_name: &str, e: tonic::Status) -> miette::Report {
    let msg = e.message().to_string();
    let is_tls = url.starts_with("https://");

    if is_tls
        && (msg.contains("h2 protocol error")
            || msg.contains("connection error")
            || msg.contains("certificate")
            || msg.contains("tls")
            || msg.contains("ssl"))
    {
        miette::miette!(
            help = "The server's TLS certificate may not be trusted by this client.\n\
                    - If using Let's Encrypt staging, switch to http:// for testing\n\
                    - If the server has no valid certificate yet, use http://<host>:<port>\n\
                    - If using a private CA, add it to your system trust store",
            "TLS connection to {} failed during {} RPC: {}",
            url,
            rpc_name,
            msg
        )
    } else if msg.contains("connection refused") || msg.contains("Connection refused") {
        miette::miette!(
            help = "Check that the forge server is running at {}",
            "{} RPC failed: connection refused",
            url
        )
    } else {
        miette::miette!("{} RPC failed: {}", rpc_name, msg)
    }
}

// ============================================================
// OAuth 2.0 Device Authorization Grant (RFC 8628)
// ============================================================

/// A stored set of OAuth tokens for a specific forge host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub issuer_url: String,
    pub client_id: String,
}

impl TokenSet {
    /// Returns true if the access token has expired (or will in < 30 seconds).
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at - chrono::Duration::seconds(30)
    }
}

/// OIDC discovery document (subset).
#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    device_authorization_endpoint: Option<String>,
    token_endpoint: String,
}

/// Response from the device authorization endpoint.
/// Some providers (e.g., Barycenter) support auto-registration and return
/// client_id/client_secret in the response when client_id is omitted from the request.
#[derive(Debug, Deserialize)]
struct DeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default = "default_poll_interval")]
    interval: u64,
    expires_in: u64,
    /// Auto-registered client ID (returned by providers that support dynamic registration)
    #[serde(default)]
    client_id: Option<String>,
    /// Auto-registered client secret
    #[serde(default)]
    client_secret: Option<String>,
}

fn default_poll_interval() -> u64 {
    5
}

/// Response from the token endpoint (success or error).
#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Fetch the OIDC configuration (issuer_url, client_id) from the forge gRPC server.
pub async fn get_auth_config(
    grpc_url: &str,
    tls_insecure: bool,
) -> miette::Result<(String, String)> {
    let channel = connect_grpc(grpc_url, tls_insecure).await?;
    let mut client = api_v2::auth_service_client::AuthServiceClient::new(channel);
    let resp = client
        .get_auth_config(Request::new(api_v2::GetAuthConfigRequest {}))
        .await
        .map_err(|e| diagnose_rpc_error(grpc_url, "GetAuthConfig", e))?;
    let inner = resp.into_inner();
    if inner.issuer_url.is_empty() {
        return Err(miette::miette!(
            "forge server returned empty issuer_url; OIDC may not be configured"
        ));
    }
    Ok((inner.issuer_url, inner.client_id))
}

/// Run the full OAuth 2.0 Device Authorization Grant flow (RFC 8628).
///
/// 1. Fetches OIDC configuration from the forge server via gRPC.
/// 2. Discovers the provider's device authorization and token endpoints.
/// 3. Initiates a device authorization request.
/// 4. Displays the user code and verification URL for the user to open in a browser.
/// 5. Polls the token endpoint until the user authorizes or the code expires.
pub async fn login_device_flow(forge_host: &str, tls_insecure: bool) -> miette::Result<TokenSet> {
    let grpc_url = server_url_from_host(forge_host);

    // 1. Get auth config from forge
    info!(host = %forge_host, "fetching OIDC configuration from forge server");
    let (issuer_url, client_id) = get_auth_config(&grpc_url, tls_insecure).await?;
    debug!(issuer = %issuer_url, client_id = %client_id, "received auth config");

    // 2. OIDC discovery
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    );
    let http = reqwest::Client::new();
    let discovery: OidcDiscovery = http
        .get(&discovery_url)
        .send()
        .await
        .map_err(|e| {
            miette::miette!(
                help = format!(
                    "Ensure the OIDC issuer at {} is reachable and supports OpenID Connect Discovery",
                    issuer_url
                ),
                "failed to fetch OIDC discovery document: {}",
                e
            )
        })?
        .json()
        .await
        .map_err(|e| {
            miette::miette!(
                "failed to parse OIDC discovery document from {}: {}",
                discovery_url,
                e
            )
        })?;

    let device_auth_endpoint = discovery.device_authorization_endpoint.ok_or_else(|| {
        miette::miette!(
            help = "The OIDC provider must support RFC 8628 (Device Authorization Grant). \
                    Check that the provider's configuration includes a device_authorization_endpoint.",
            "OIDC provider at {} does not advertise a device_authorization_endpoint",
            issuer_url
        )
    })?;

    // 3. Request device authorization
    // Try with client_id first; if the provider rejects it (e.g., unregistered client),
    // retry without client_id to trigger auto-registration (RFC 7591 / provider-specific).
    debug!(endpoint = %device_auth_endpoint, "requesting device authorization");

    let device_resp = {
        let mut form_params: Vec<(&str, &str)> = vec![("scope", "openid profile email offline_access")];
        if !client_id.is_empty() {
            form_params.push(("client_id", &client_id));
        }

        let resp = http
            .post(&device_auth_endpoint)
            .form(&form_params)
            .send()
            .await
            .map_err(|e| {
                miette::miette!(
                    "device authorization request to {} failed: {}",
                    device_auth_endpoint,
                    e
                )
            })?;

        if !resp.status().is_success() && !client_id.is_empty() {
            // Retry without client_id for providers that support auto-registration
            info!("device authorization with client_id failed, retrying with auto-registration");
            let resp = http
                .post(&device_auth_endpoint)
                .form(&[("scope", "openid profile email offline_access")])
                .send()
                .await
                .map_err(|e| {
                    miette::miette!(
                        "device authorization request to {} failed: {}",
                        device_auth_endpoint,
                        e
                    )
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(miette::miette!(
                    "device authorization failed (HTTP {}):\n  {}\n\n\
                     The OIDC provider rejected both client_id='{}' and auto-registration.",
                    status,
                    body,
                    client_id,
                ));
            }
            resp
        } else if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(miette::miette!(
                "device authorization failed (HTTP {}):\n  {}",
                status,
                body,
            ));
        } else {
            resp
        }
    };

    let device_resp: DeviceAuthResponse = device_resp
        .json()
        .await
        .map_err(|e| miette::miette!("failed to parse device authorization response: {}", e))?;

    // Use auto-registered client_id if the provider returned one
    let effective_client_id = device_resp
        .client_id
        .as_deref()
        .unwrap_or(&client_id)
        .to_string();
    let effective_client_secret = device_resp.client_secret.clone();

    // 4. Display instructions
    eprintln!();
    eprintln!("To authenticate, visit:");
    eprintln!("  {}", device_resp.verification_uri);
    eprintln!();
    eprintln!("And enter the code: {}", device_resp.user_code);
    eprintln!();
    eprintln!(
        "Waiting for authorization (expires in {} seconds)...",
        device_resp.expires_in
    );

    // 5. Poll for token
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(device_resp.expires_in);
    let mut interval = device_resp.interval;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

        if std::time::Instant::now() > deadline {
            return Err(miette::miette!(
                help = "Run the login command again to get a new device code.",
                "device authorization timed out after {} seconds",
                device_resp.expires_in
            ));
        }

        let mut token_form: Vec<(&str, &str)> = vec![
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", &device_resp.device_code),
            ("client_id", &effective_client_id),
        ];
        if let Some(secret) = &effective_client_secret {
            token_form.push(("client_secret", secret));
        }
        let resp = http
            .post(&discovery.token_endpoint)
            .form(&token_form)
            .send()
            .await
            .map_err(|e| {
                miette::miette!(
                    "token endpoint request to {} failed: {}",
                    discovery.token_endpoint,
                    e
                )
            })?;

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| miette::miette!("failed to parse token response: {}", e))?;

        match token_resp.error.as_deref() {
            Some("authorization_pending") => {
                debug!("authorization pending, polling again");
                continue;
            }
            Some("slow_down") => {
                // RFC 8628 section 3.5: increase interval by 5 seconds
                interval += 5;
                debug!(new_interval = interval, "slow_down received, backing off");
                continue;
            }
            Some(err) => {
                let desc = token_resp
                    .error_description
                    .unwrap_or_else(|| err.to_string());
                return Err(miette::miette!(
                    help = "Check that you authorized the correct device code in your browser.",
                    "authorization failed: {} ({})",
                    desc,
                    err
                ));
            }
            None => {
                // Success
                let access_token = token_resp
                    .access_token
                    .ok_or_else(|| miette::miette!("token response missing access_token field"))?;
                let expires_in = token_resp.expires_in.unwrap_or(3600);
                let expires_at = Utc::now() + chrono::Duration::seconds(expires_in as i64);

                info!("device authorization successful");
                return Ok(TokenSet {
                    access_token,
                    refresh_token: token_resp.refresh_token,
                    expires_at,
                    issuer_url,
                    client_id,
                });
            }
        }
    }
}

/// Refresh an OAuth token using the refresh_token grant.
pub async fn refresh_token(token_set: &TokenSet) -> miette::Result<TokenSet> {
    let refresh = token_set.refresh_token.as_deref().ok_or_else(|| {
        miette::miette!(
            help = "Run 'pkgdev auth login' to obtain a new token.",
            "no refresh token available; cannot refresh"
        )
    })?;

    // Discover token endpoint
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        token_set.issuer_url.trim_end_matches('/')
    );
    let http = reqwest::Client::new();
    let discovery: OidcDiscovery = http
        .get(&discovery_url)
        .send()
        .await
        .map_err(|e| miette::miette!("failed to fetch OIDC discovery: {}", e))?
        .json()
        .await
        .map_err(|e| miette::miette!("failed to parse OIDC discovery: {}", e))?;

    let resp = http
        .post(&discovery.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", &token_set.client_id),
        ])
        .send()
        .await
        .map_err(|e| miette::miette!("token refresh request failed: {}", e))?;

    let token_resp: TokenResponse = resp
        .json()
        .await
        .map_err(|e| miette::miette!("failed to parse token refresh response: {}", e))?;

    if let Some(err) = &token_resp.error {
        let desc = token_resp
            .error_description
            .as_deref()
            .unwrap_or(err.as_str());
        return Err(miette::miette!(
            help =
                "Your refresh token may have expired. Run 'pkgdev auth login' to re-authenticate.",
            "token refresh failed: {} ({})",
            desc,
            err
        ));
    }

    let access_token = token_resp
        .access_token
        .ok_or_else(|| miette::miette!("refresh response missing access_token"))?;
    let expires_in = token_resp.expires_in.unwrap_or(3600);
    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in as i64);

    Ok(TokenSet {
        access_token,
        refresh_token: token_resp
            .refresh_token
            .or_else(|| token_set.refresh_token.clone()),
        expires_at,
        issuer_url: token_set.issuer_url.clone(),
        client_id: token_set.client_id.clone(),
    })
}

// ============================================================
// Token Store — persisted in $XDG_DATA_HOME/pkgdev/tokens.json
// ============================================================

/// Persistent store for OAuth tokens keyed by forge host.
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenStore {
    tokens: HashMap<String, TokenSet>,
    #[serde(skip)]
    path: PathBuf,
}

impl TokenStore {
    /// Load the token store from the default path, or return an empty store.
    pub fn load() -> Self {
        let path = default_token_store_path();
        if path.exists() {
            match std::fs::read(&path) {
                Ok(bytes) => match serde_json::from_slice::<TokenStore>(&bytes) {
                    Ok(mut store) => {
                        store.path = path;
                        return store;
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "corrupt token store, starting fresh");
                    }
                },
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "unable to read token store");
                }
            }
        }
        TokenStore {
            tokens: HashMap::new(),
            path,
        }
    }

    /// Save the token store to disk with restrictive permissions (0600).
    pub fn save(&self) -> miette::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                miette::miette!(
                    "failed to create token store directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
        let buf = serde_json::to_vec_pretty(self)
            .map_err(|e| miette::miette!("failed to serialize token store: {}", e))?;
        std::fs::write(&self.path, &buf).map_err(|e| {
            miette::miette!(
                "failed to write token store to {}: {}",
                self.path.display(),
                e
            )
        })?;

        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.path, perms).map_err(|e| {
                miette::miette!(
                    "failed to set permissions on {}: {}",
                    self.path.display(),
                    e
                )
            })?;
        }

        debug!(path = %self.path.display(), "token store saved");
        Ok(())
    }

    /// Get the token set for a given host, if one exists.
    pub fn get(&self, host: &str) -> Option<&TokenSet> {
        self.tokens.get(host)
    }

    /// Insert or update the token set for a host.
    pub fn set(&mut self, host: String, token: TokenSet) {
        self.tokens.insert(host, token);
    }

    /// Remove tokens for a host.
    pub fn remove(&mut self, host: &str) -> Option<TokenSet> {
        self.tokens.remove(host)
    }

    /// List all stored hosts.
    pub fn hosts(&self) -> Vec<&str> {
        self.tokens.keys().map(|s| s.as_str()).collect()
    }
}

fn default_token_store_path() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("pkgdev").join("tokens.json")
    } else if let Ok(pd) = crate::get_project_dir() {
        pd.data_dir().join("tokens.json")
    } else {
        PathBuf::from("tokens.json")
    }
}

/// Build a `tonic::Request` with a Bearer token in the `authorization` metadata.
pub fn authenticated_request<T>(inner: T, token: &str) -> tonic::Request<T> {
    let mut req = tonic::Request::new(inner);
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token)
            .parse()
            .expect("valid bearer header"),
    );
    req
}

/// Get a valid access token for the given host.
///
/// Loads from the token store. If the token is expired and a refresh token is
/// available, attempts to refresh it. Saves back to the store on refresh.
///
/// Returns the access token string ready for use as a Bearer token.
pub async fn get_valid_token(host: &str) -> miette::Result<String> {
    let mut store = TokenStore::load();
    let token_set = store.get(host).ok_or_else(|| {
        miette::miette!(
            help = format!(
                "Run 'pkgdev auth login --host {}' to authenticate first.",
                host
            ),
            "no stored token for host '{}'",
            host
        )
    })?;

    if !token_set.is_expired() {
        return Ok(token_set.access_token.clone());
    }

    info!(host = %host, "access token expired, attempting refresh");
    let token_set_owned = token_set.clone();
    let refreshed = refresh_token(&token_set_owned).await?;
    let access = refreshed.access_token.clone();
    store.set(host.to_string(), refreshed);
    store.save()?;
    Ok(access)
}

/// Display the token status for a host (or all hosts).
pub fn print_token_status(host: Option<&str>) {
    let store = TokenStore::load();
    let hosts: Vec<&str> = if let Some(h) = host {
        if store.get(h).is_some() {
            vec![h]
        } else {
            println!("no stored token for host '{}'", h);
            return;
        }
    } else {
        let mut h = store.hosts();
        h.sort();
        h
    };

    if hosts.is_empty() {
        println!("no stored tokens");
        return;
    }

    for h in hosts {
        if let Some(ts) = store.get(h) {
            let status = if ts.is_expired() { "EXPIRED" } else { "valid" };
            let has_refresh = if ts.refresh_token.is_some() {
                "yes"
            } else {
                "no"
            };
            println!(
                "{}\t{}\texpires={}\trefresh={}",
                h, status, ts.expires_at, has_refresh
            );
        }
    }
}
