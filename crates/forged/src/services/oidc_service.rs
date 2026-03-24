use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use miette::{Context, IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// OIDC Service for token validation
/// Performs full OIDC discovery, JWKS fetching, and JWT signature validation.
#[derive(Clone)]
pub struct OidcService {
    issuer_url: String,
    client_id: String,
    audience: String,
    http_client: reqwest::Client,
    /// Cached JWKS keys (fetched lazily on first validation)
    jwks_cache: Arc<RwLock<Option<JwksCache>>>,
}

#[derive(Clone)]
struct JwksCache {
    keys: Vec<JwkKey>,
    fetched_at: std::time::Instant,
}

#[derive(Debug, Clone, Deserialize)]
struct JwkKey {
    kid: Option<String>,
    kty: String,
    alg: Option<String>,
    n: Option<String>,
    e: Option<String>,
    #[serde(rename = "use")]
    key_use: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    issuer: String,
    jwks_uri: String,
}

/// Standard OIDC JWT claims
#[derive(Debug, Deserialize, Serialize)]
struct JwtClaims {
    sub: String,
    iss: Option<String>,
    aud: Option<AudClaim>,
    exp: Option<u64>,
    iat: Option<u64>,
    name: Option<String>,
    preferred_username: Option<String>,
    email: Option<String>,
}

/// Audience can be a string or array of strings
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum AudClaim {
    Single(String),
    Multiple(Vec<String>),
}

impl AudClaim {
    #[allow(dead_code)]
    fn contains(&self, aud: &str) -> bool {
        match self {
            AudClaim::Single(s) => s == aud,
            AudClaim::Multiple(v) => v.iter().any(|s| s == aud),
        }
    }
}

impl OidcService {
    pub fn new(issuer_url: String, client_id: String, audience: String) -> Self {
        Self {
            issuer_url,
            client_id,
            audience,
            http_client: reqwest::Client::new(),
            jwks_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Validate an OIDC token and extract claims.
    ///
    /// 1. Fetch OIDC discovery document from issuer (cached)
    /// 2. Fetch JWKS and find matching key (cached)
    /// 3. Validate JWT signature, issuer, audience, expiration
    /// 4. Extract subject and display name
    pub async fn validate_token(&self, token: &str) -> Result<OidcClaims> {
        if token.is_empty() {
            return Err(miette::miette!(
                "OIDC token is required.\n\
                 Obtain a token from your OIDC provider and include it in the Authorization header."
            ));
        }

        // If OIDC is not configured (empty issuer), fall back to stub mode for development
        if self.issuer_url.is_empty() {
            return self.validate_stub(token);
        }

        // 1. Try to decode JWT header. If this fails, the token may be opaque —
        //    fall back to userinfo endpoint validation.
        let header = match decode_header(token) {
            Ok(h) => h,
            Err(_) => {
                tracing::debug!("Token is not a JWT, attempting userinfo endpoint validation");
                return self.validate_via_userinfo(token).await;
            }
        };

        let kid = header.kid.as_deref();
        let alg = header.alg;

        // 2. Get JWKS keys (with caching)
        let jwk = self.find_jwk(kid, alg).await?;

        // 3. Build decoding key from JWK
        let decoding_key = match (jwk.n.as_ref(), jwk.e.as_ref()) {
            (Some(n), Some(e)) => DecodingKey::from_rsa_components(n, e)
                .into_diagnostic()
                .wrap_err(
                    "Failed to construct RSA key from JWKS.\n\
                     The OIDC provider's JWKS may be malformed.",
                )?,
            _ => {
                return Err(miette::miette!(
                    "Unsupported key type in JWKS: {}.\n\
                     Only RSA keys are currently supported.",
                    jwk.kty
                ));
            }
        };

        // 4. Validate and decode token
        let mut validation = Validation::new(alg);
        validation.set_issuer(&[&self.issuer_url]);
        if !self.audience.is_empty() {
            validation.set_audience(&[&self.audience]);
        }
        validation.validate_exp = true;

        let token_data = decode::<JwtClaims>(token, &decoding_key, &validation)
            .into_diagnostic()
            .wrap_err(
                "OIDC token validation failed.\n\
                 The token may be expired, issued by a different provider, or intended for a different audience.\n\
                 Obtain a fresh token from your OIDC provider.",
            )?;

        let claims = token_data.claims;

        // 5. Extract display name (prefer name > preferred_username > sub)
        let display_name = claims
            .name
            .clone()
            .or_else(|| claims.preferred_username.clone())
            .unwrap_or_else(|| claims.sub.clone());

        Ok(OidcClaims {
            subject: claims.sub,
            display_name,
            email: claims.email,
        })
    }

    /// Validate an opaque (non-JWT) token by calling the OIDC provider's userinfo endpoint.
    /// This is the standard way to validate opaque access tokens per RFC 6750.
    async fn validate_via_userinfo(&self, token: &str) -> Result<OidcClaims> {
        // Discover the userinfo endpoint
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            self.issuer_url.trim_end_matches('/')
        );
        let discovery: serde_json::Value = self
            .http_client
            .get(&discovery_url)
            .send()
            .await
            .into_diagnostic()
            .wrap_err("Failed to fetch OIDC discovery for userinfo endpoint")?
            .json()
            .await
            .into_diagnostic()
            .wrap_err("Failed to parse OIDC discovery document")?;

        let userinfo_endpoint = discovery["userinfo_endpoint"]
            .as_str()
            .ok_or_else(|| miette::miette!("OIDC provider does not have a userinfo_endpoint"))?;

        // Call userinfo with the opaque token as Bearer
        let resp = self
            .http_client
            .get(userinfo_endpoint)
            .bearer_auth(token)
            .send()
            .await
            .into_diagnostic()
            .wrap_err("Userinfo request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(miette::miette!(
                "Userinfo endpoint returned {}: {}\n\
                 The access token may be invalid or expired.",
                status,
                body
            ));
        }

        let userinfo: serde_json::Value = resp
            .json()
            .await
            .into_diagnostic()
            .wrap_err("Failed to parse userinfo response")?;

        let sub = userinfo["sub"]
            .as_str()
            .ok_or_else(|| miette::miette!("Userinfo response missing 'sub' claim"))?
            .to_string();

        let display_name = userinfo["name"]
            .as_str()
            .or_else(|| userinfo["preferred_username"].as_str())
            .unwrap_or(&sub)
            .to_string();

        let email = userinfo["email"].as_str().map(|s| s.to_string());

        tracing::info!(sub = %sub, name = %display_name, "Validated opaque token via userinfo");

        Ok(OidcClaims {
            subject: sub,
            display_name,
            email,
        })
    }

    /// Stub validation for development (when issuer_url is empty)
    fn validate_stub(&self, token: &str) -> Result<OidcClaims> {
        tracing::warn!(
            "OIDC not configured — using stub token validation. Do NOT use in production."
        );

        if let Some((sub, name)) = token.split_once(':') {
            Ok(OidcClaims {
                subject: sub.to_string(),
                display_name: name.to_string(),
                email: None,
            })
        } else {
            Ok(OidcClaims {
                subject: token.to_string(),
                display_name: token.to_string(),
                email: None,
            })
        }
    }

    /// Find a JWK matching the given kid and algorithm.
    /// If the kid is not found in the cache, forces a JWKS refresh before failing.
    async fn find_jwk(&self, kid: Option<&str>, alg: Algorithm) -> Result<JwkKey> {
        let keys = self.get_jwks().await?;

        if let Some(found) = Self::match_jwk(&keys, kid, alg) {
            return Ok(found);
        }

        // Key not found — the provider may have rotated keys.
        // Force a JWKS refresh and try once more.
        if kid.is_some() {
            tracing::info!(kid = ?kid, "JWK not found in cache, forcing JWKS refresh");
            let fresh_keys = self.fetch_jwks().await?;
            {
                let mut cache = self.jwks_cache.write().await;
                *cache = Some(JwksCache {
                    keys: fresh_keys.clone(),
                    fetched_at: std::time::Instant::now(),
                });
            }
            if let Some(found) = Self::match_jwk(&fresh_keys, kid, alg) {
                return Ok(found);
            }
        }

        Err(miette::miette!(
            "No suitable signing key found in JWKS from {}.\n\
             The OIDC provider may not have published RSA signing keys.\n\
             Token kid: {:?}, algorithm: {:?}",
            self.issuer_url,
            kid,
            alg
        ))
    }

    /// Try to find a matching JWK from a set of keys.
    fn match_jwk(keys: &[JwkKey], kid: Option<&str>, alg: Algorithm) -> Option<JwkKey> {
        // Match by kid first
        if let Some(kid) = kid {
            if let Some(key) = keys.iter().find(|k| k.kid.as_deref() == Some(kid)) {
                return Some(key.clone());
            }
        }

        // Fall back to matching by algorithm and use=sig
        let alg_str = format!("{:?}", alg);
        if let Some(key) = keys.iter().find(|k| {
            k.key_use.as_deref() == Some("sig")
                && k.alg.as_deref().map(|a| a == alg_str).unwrap_or(true)
        }) {
            return Some(key.clone());
        }

        // Last resort: first RSA key
        keys.iter().find(|k| k.kty == "RSA").cloned()
    }

    /// Get JWKS keys, using cache if available and fresh (< 1 hour)
    async fn get_jwks(&self) -> Result<Vec<JwkKey>> {
        // Check cache
        {
            let cache = self.jwks_cache.read().await;
            if let Some(ref c) = *cache {
                if c.fetched_at.elapsed() < std::time::Duration::from_secs(3600) {
                    return Ok(c.keys.clone());
                }
            }
        }

        // Fetch fresh JWKS
        let keys = self.fetch_jwks().await?;

        // Update cache
        {
            let mut cache = self.jwks_cache.write().await;
            *cache = Some(JwksCache {
                keys: keys.clone(),
                fetched_at: std::time::Instant::now(),
            });
        }

        Ok(keys)
    }

    /// Fetch JWKS from the OIDC provider
    async fn fetch_jwks(&self) -> Result<Vec<JwkKey>> {
        // 1. Discover JWKS URI
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            self.issuer_url.trim_end_matches('/')
        );

        let discovery: OidcDiscovery = self
            .http_client
            .get(&discovery_url)
            .send()
            .await
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to fetch OIDC discovery document from {}.\n\
                     Ensure the OIDC issuer URL is correct and reachable.",
                    discovery_url
                )
            })?
            .json()
            .await
            .into_diagnostic()
            .wrap_err("Failed to parse OIDC discovery document")?;

        // 2. Validate issuer matches
        if discovery.issuer != self.issuer_url {
            return Err(miette::miette!(
                "OIDC issuer mismatch: expected {}, got {}.\n\
                 Check the OIDC issuer URL in your configuration.",
                self.issuer_url,
                discovery.issuer
            ));
        }

        // 3. Fetch JWKS
        let jwks: JwksResponse = self
            .http_client
            .get(&discovery.jwks_uri)
            .send()
            .await
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "Failed to fetch JWKS from {}.\n\
                     The OIDC provider may be temporarily unavailable.",
                    discovery.jwks_uri
                )
            })?
            .json()
            .await
            .into_diagnostic()
            .wrap_err("Failed to parse JWKS response")?;

        tracing::info!(
            issuer = %self.issuer_url,
            keys = jwks.keys.len(),
            "Fetched JWKS from OIDC provider"
        );

        Ok(jwks.keys)
    }

    /// Get the configured issuer URL
    pub fn issuer_url(&self) -> &str {
        &self.issuer_url
    }

    /// Get the configured client ID
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

/// OIDC token claims extracted after validation
#[derive(Debug, Clone)]
pub struct OidcClaims {
    /// Subject (unique user identifier from OIDC provider)
    pub subject: String,
    /// Display name
    pub display_name: String,
    /// Email (optional)
    pub email: Option<String>,
}
