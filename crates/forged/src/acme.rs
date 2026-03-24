//! ACME certificate management for automatic TLS via Let's Encrypt.
//!
//! Supports HTTP-01 challenge verification. On startup, checks the cache for
//! existing certificates. If missing or expired, acquires a new certificate.
//! A background task handles renewal before expiry.

use crate::settings::AcmeConfig;
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, RetryPolicy,
};
use miette::{Context, IntoDiagnostic, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Certificate and key in PEM format, ready for tonic.
#[derive(Clone)]
pub struct CertificateBundle {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
}

/// Shared state for HTTP-01 challenge tokens.
/// Maps token -> key authorization.
pub type ChallengeTokens = Arc<RwLock<HashMap<String, String>>>;

/// Manages the ACME lifecycle: account creation, certificate issuance, and renewal.
pub struct AcmeManager {
    config: AcmeConfig,
    cache_dir: PathBuf,
    challenge_tokens: ChallengeTokens,
}

impl AcmeManager {
    pub fn new(config: AcmeConfig) -> Result<Self> {
        let cache_dir = PathBuf::from(&config.cache_dir);
        std::fs::create_dir_all(&cache_dir)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to create ACME cache directory: {}\n\
                     Ensure the path is writable.",
                    cache_dir.display()
                )
            })?;

        Ok(Self {
            config,
            cache_dir,
            challenge_tokens: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Returns the shared challenge token store for the HTTP-01 responder.
    pub fn challenge_tokens(&self) -> ChallengeTokens {
        self.challenge_tokens.clone()
    }

    /// Get or issue a certificate. Returns the PEM bundle.
    ///
    /// Checks the cache first; if a valid cert exists, returns it.
    /// Otherwise, runs the ACME flow to acquire a new one.
    pub async fn ensure_certificate(&self) -> Result<CertificateBundle> {
        // Try loading from cache
        if let Some(bundle) = self.load_cached_cert()? {
            info!("Loaded TLS certificate from cache");
            return Ok(bundle);
        }

        info!(
            domains = ?self.config.domains,
            "No cached certificate found, requesting from ACME provider"
        );

        let bundle = self.issue_certificate().await?;
        self.save_cert_to_cache(&bundle)?;
        Ok(bundle)
    }

    /// Issue a new certificate via ACME.
    async fn issue_certificate(&self) -> Result<CertificateBundle> {
        // Load or create ACME account
        let account = self.get_or_create_account().await?;

        // Create order
        let identifiers: Vec<Identifier> = self
            .config
            .domains
            .iter()
            .map(|d| Identifier::Dns(d.clone()))
            .collect();

        let mut order = account
            .new_order(&NewOrder::new(&identifiers))
            .await
            .into_diagnostic()
            .wrap_err("Failed to create ACME order")?;

        // Process authorizations
        let mut auths = order.authorizations();
        while let Some(auth_result) = auths.next().await {
            let mut auth = auth_result
                .into_diagnostic()
                .wrap_err("Failed to fetch ACME authorization")?;

            match auth.status {
                AuthorizationStatus::Valid => continue,
                AuthorizationStatus::Pending => {}
                status => {
                    return Err(miette::miette!(
                        "Unexpected authorization status: {:?}\n\
                         This may indicate a previous failed attempt. Try again later.",
                        status
                    ));
                }
            }

            // Get the HTTP-01 challenge
            let mut challenge = auth.challenge(ChallengeType::Http01).ok_or_else(|| {
                miette::miette!(
                    "No HTTP-01 challenge available for domain.\n\
                     Ensure the ACME provider supports HTTP-01 challenges\n\
                     and that port 80 is accessible from the internet."
                )
            })?;

            // Compute key authorization and publish it
            let key_auth = challenge.key_authorization();
            {
                let mut tokens = self.challenge_tokens.write().await;
                tokens.insert(challenge.token.clone(), key_auth.as_str().to_string());
            }

            // Tell ACME server we're ready
            challenge
                .set_ready()
                .await
                .into_diagnostic()
                .wrap_err("Failed to signal challenge readiness to ACME server")?;
        }
        // Let the authorizations borrow go out of scope before using order again
        let _ = auths;

        // Wait for order to become ready
        let retries = RetryPolicy::new().timeout(Duration::from_secs(120));
        order
            .poll_ready(&retries)
            .await
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "ACME order did not become ready.\n\
                     Check that port 80 is accessible from the internet\n\
                     and that DNS for {:?} resolves to this server.",
                    self.config.domains
                )
            })?;

        // Clean up challenge tokens
        {
            let mut tokens = self.challenge_tokens.write().await;
            tokens.clear();
        }

        // Finalize the order — this generates a CSR internally and returns the PEM key
        let key_pem = order
            .finalize()
            .await
            .into_diagnostic()
            .wrap_err("Failed to finalize ACME order")?;

        // Poll for certificate
        let cert_pem = order
            .poll_certificate(&retries)
            .await
            .into_diagnostic()
            .wrap_err("Failed to retrieve certificate from ACME provider")?;

        info!(
            domains = ?self.config.domains,
            "Successfully acquired TLS certificate from ACME provider"
        );

        Ok(CertificateBundle {
            cert_pem: cert_pem.into_bytes(),
            key_pem: key_pem.into_bytes(),
        })
    }

    /// Load or create an ACME account.
    async fn get_or_create_account(&self) -> Result<Account> {
        let creds_path = self.cache_dir.join("account_credentials.json");

        if creds_path.exists() {
            let data = std::fs::read_to_string(&creds_path)
                .into_diagnostic()
                .wrap_err("Failed to read ACME account credentials from cache")?;
            let creds: AccountCredentials =
                serde_json::from_str(&data).into_diagnostic().wrap_err(
                    "Failed to parse cached ACME account credentials.\n\
                     Try deleting the cache and re-running.",
                )?;
            let account = Account::builder()
                .into_diagnostic()
                .wrap_err("Failed to create ACME account builder")?
                .from_credentials(creds)
                .await
                .into_diagnostic()
                .wrap_err("Failed to restore ACME account from cached credentials")?;
            info!("Loaded ACME account from cache");
            return Ok(account);
        }

        info!("Creating new ACME account");
        let contact: Vec<&str> = self.config.contact.iter().map(|s| s.as_str()).collect();
        let (account, creds) = Account::builder()
            .into_diagnostic()
            .wrap_err("Failed to create ACME account builder")?
            .create(
                &NewAccount {
                    contact: &contact,
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                self.config.directory_url.clone(),
                None,
            )
            .await
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to create ACME account at {}.\n\
                     Check that the ACME directory URL is reachable.",
                    self.config.directory_url
                )
            })?;

        // Save credentials
        let data = serde_json::to_string_pretty(&creds)
            .into_diagnostic()
            .wrap_err("Failed to serialize ACME account credentials")?;
        std::fs::write(&creds_path, data)
            .into_diagnostic()
            .wrap_err("Failed to save ACME account credentials to cache")?;

        Ok(account)
    }

    fn cert_path(&self) -> PathBuf {
        self.cache_dir.join("cert.pem")
    }

    fn key_path(&self) -> PathBuf {
        self.cache_dir.join("key.pem")
    }

    fn load_cached_cert(&self) -> Result<Option<CertificateBundle>> {
        let cert_path = self.cert_path();
        let key_path = self.key_path();

        if !cert_path.exists() || !key_path.exists() {
            return Ok(None);
        }

        let cert_pem = std::fs::read(&cert_path)
            .into_diagnostic()
            .wrap_err("Failed to read cached certificate")?;
        let key_pem = std::fs::read(&key_path)
            .into_diagnostic()
            .wrap_err("Failed to read cached key")?;

        // Check if the certificate is still valid (has at least 30 days remaining)
        if is_cert_expiring_soon(&cert_pem, 30) {
            warn!("Cached certificate expires within 30 days, will renew");
            return Ok(None);
        }

        Ok(Some(CertificateBundle { cert_pem, key_pem }))
    }

    fn save_cert_to_cache(&self, bundle: &CertificateBundle) -> Result<()> {
        std::fs::write(self.cert_path(), &bundle.cert_pem)
            .into_diagnostic()
            .wrap_err("Failed to save certificate to cache")?;
        std::fs::write(self.key_path(), &bundle.key_pem)
            .into_diagnostic()
            .wrap_err("Failed to save key to cache")?;
        Ok(())
    }
}

/// Check if a PEM-encoded certificate expires within `days` days.
fn is_cert_expiring_soon(pem_data: &[u8], days: u32) -> bool {
    use rustls_pemfile::certs;
    use std::io::Cursor;

    let mut cursor = Cursor::new(pem_data);
    let certs: Vec<_> = certs(&mut cursor).filter_map(|r| r.ok()).collect();

    if certs.is_empty() {
        return true; // No certs found = treat as expired
    }

    // Parse the first certificate to check expiry
    match x509_not_after(&certs[0]) {
        Some(not_after) => {
            let threshold = chrono::Utc::now() + chrono::Duration::days(i64::from(days));
            not_after < threshold
        }
        None => true,
    }
}

/// Extract the notAfter timestamp from a DER-encoded X.509 certificate.
/// Returns None if parsing fails (safe default: will trigger renewal).
fn x509_not_after(der: &[u8]) -> Option<chrono::DateTime<chrono::Utc>> {
    parse_x509_validity(der)
}

fn parse_x509_validity(der: &[u8]) -> Option<chrono::DateTime<chrono::Utc>> {
    let mut pos = 0;
    // Skip outer SEQUENCE
    pos = skip_tag_length(der, pos, 0x30)?;
    // Skip TBSCertificate SEQUENCE tag+length
    pos = skip_tag_length(der, pos, 0x30)?;
    // Skip version (context [0]) if present
    if der.get(pos).copied()? & 0xe0 == 0xa0 {
        pos = skip_tlv(der, pos)?;
    }
    // Skip serialNumber
    pos = skip_tlv(der, pos)?;
    // Skip signature algorithm
    pos = skip_tlv(der, pos)?;
    // Skip issuer
    pos = skip_tlv(der, pos)?;
    // Now at validity SEQUENCE
    pos = skip_tag_length(der, pos, 0x30)?;
    // Skip notBefore
    pos = skip_tlv(der, pos)?;
    // Parse notAfter
    parse_asn1_time(der, pos)
}

fn skip_tag_length(der: &[u8], pos: usize, expected_tag: u8) -> Option<usize> {
    if *der.get(pos)? != expected_tag {
        return None;
    }
    skip_length(der, pos + 1)
}

fn skip_length(der: &[u8], pos: usize) -> Option<usize> {
    let b = *der.get(pos)?;
    if b < 0x80 {
        Some(pos + 1)
    } else {
        let num_bytes = (b & 0x7f) as usize;
        Some(pos + 1 + num_bytes)
    }
}

fn get_length(der: &[u8], pos: usize) -> Option<(usize, usize)> {
    let b = *der.get(pos)?;
    if b < 0x80 {
        Some((b as usize, pos + 1))
    } else {
        let num_bytes = (b & 0x7f) as usize;
        let mut len = 0usize;
        for i in 0..num_bytes {
            len = (len << 8) | (*der.get(pos + 1 + i)? as usize);
        }
        Some((len, pos + 1 + num_bytes))
    }
}

fn skip_tlv(der: &[u8], pos: usize) -> Option<usize> {
    let _tag = *der.get(pos)?;
    let (len, data_start) = get_length(der, pos + 1)?;
    Some(data_start + len)
}

fn parse_asn1_time(der: &[u8], pos: usize) -> Option<chrono::DateTime<chrono::Utc>> {
    let tag = *der.get(pos)?;
    let (len, data_start) = get_length(der, pos + 1)?;
    let time_str = std::str::from_utf8(der.get(data_start..data_start + len)?).ok()?;

    match tag {
        0x17 => {
            // UTCTime: YYMMDDHHMMSSZ
            if time_str.len() == 13 {
                chrono::NaiveDateTime::parse_from_str(time_str, "%y%m%d%H%M%SZ")
                    .ok()
                    .map(|dt| dt.and_utc())
            } else {
                None
            }
        }
        0x18 => {
            // GeneralizedTime: YYYYMMDDHHMMSSZ
            chrono::NaiveDateTime::parse_from_str(time_str, "%Y%m%d%H%M%SZ")
                .ok()
                .map(|dt| dt.and_utc())
        }
        _ => None,
    }
}

/// Spawn a background task that renews the certificate before it expires.
pub fn spawn_renewal_task(manager: Arc<AcmeManager>, cancel: tokio_util::sync::CancellationToken) {
    tokio::spawn(async move {
        // Check every 12 hours
        let check_interval = Duration::from_secs(12 * 3600);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("ACME renewal task shutting down");
                    break;
                }
                _ = tokio::time::sleep(check_interval) => {
                    info!("Checking certificate for renewal");
                    match manager.ensure_certificate().await {
                        Ok(_) => info!("Certificate is up to date"),
                        Err(e) => error!(error = ?e, "Certificate renewal failed"),
                    }
                }
            }
        }
    });
}

/// Start an HTTP server on port 80 to handle ACME HTTP-01 challenges.
///
/// Responds to `GET /.well-known/acme-challenge/{token}` with the key authorization.
/// All other requests get a 404.
pub async fn start_http01_challenge_server(
    listen_addr: std::net::SocketAddr,
    tokens: ChallengeTokens,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<()> {
    use http_body_util::Full;
    use hyper::body::Bytes;
    use hyper::Request;
    use hyper::Response;
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;

    let listener = tokio::net::TcpListener::bind(listen_addr)
        .await
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "Failed to bind HTTP-01 challenge server on {}.\n\
                 Port 80 must be available for ACME HTTP-01 challenges.\n\
                 Ensure no other service is using port 80.",
                listen_addr
            )
        })?;

    info!(%listen_addr, "ACME HTTP-01 challenge server listening");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("HTTP-01 challenge server shutting down");
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _)) => {
                        let tokens = tokens.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let service = hyper::service::service_fn(move |req: Request<hyper::body::Incoming>| {
                                let tokens = tokens.clone();
                                async move {
                                    let path = req.uri().path();
                                    if let Some(token) = path.strip_prefix("/.well-known/acme-challenge/") {
                                        let store = tokens.read().await;
                                        if let Some(auth) = store.get(token) {
                                            return Ok::<_, Infallible>(
                                                Response::new(Full::new(Bytes::from(auth.clone())))
                                            );
                                        }
                                    }
                                    Ok(Response::builder()
                                        .status(404)
                                        .body(Full::new(Bytes::from("not found")))
                                        .unwrap())
                                }
                            });
                            if let Err(e) = hyper::server::conn::http1::Builder::new()
                                .serve_connection(io, service)
                                .await
                            {
                                tracing::debug!(error = %e, "HTTP-01 connection error");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "HTTP-01 accept error");
                    }
                }
            }
        }
    }

    Ok(())
}
