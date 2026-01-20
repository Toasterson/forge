use miette::{Context, IntoDiagnostic, Result};

/// OIDC Service for token validation
/// TODO: Replace with actual OIDC integration using your existing OIDC project
#[derive(Clone)]
pub struct OidcService {
    issuer_url: String,
    client_id: String,
    audience: String,
}

impl OidcService {
    pub fn new(issuer_url: String, client_id: String, audience: String) -> Self {
        Self {
            issuer_url,
            client_id,
            audience,
        }
    }

    /// Validate an OIDC token and extract claims
    /// Returns (oidc_sub, display_name)
    ///
    /// TODO: Implement actual OIDC validation
    /// This should:
    /// 1. Fetch OIDC discovery document from issuer
    /// 2. Validate token signature using JWKS
    /// 3. Verify token claims (iss, aud, exp, etc.)
    /// 4. Extract subject and display name from claims
    pub async fn validate_token(&self, token: &str) -> Result<OidcClaims> {
        if token.is_empty() {
            return Err(miette::miette!("OIDC token is required"));
        }

        // Stub implementation for MVP
        // Expected format for testing: "oidc_sub:display_name"

        if let Some((sub, name)) = token.split_once(':') {
            Ok(OidcClaims {
                subject: sub.to_string(),
                display_name: name.to_string(),
                email: None,
            })
        } else {
            // For simple tokens, use the token as both sub and display name
            Ok(OidcClaims {
                subject: token.to_string(),
                display_name: token.to_string(),
                email: None,
            })
        }
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

/// OIDC token claims
#[derive(Debug, Clone)]
pub struct OidcClaims {
    /// Subject (unique user identifier)
    pub subject: String,
    /// Display name
    pub display_name: String,
    /// Email (optional)
    pub email: Option<String>,
}

/* TODO: Real OIDC implementation example using openid crate:

use openid::{Client, Discovered, StandardClaims, Token};
use reqwest::Url;

impl OidcService {
    pub async fn new(issuer_url: String, client_id: String, audience: String) -> Result<Self> {
        // Discover OIDC configuration
        let issuer = Url::parse(&issuer_url)
            .into_diagnostic()
            .wrap_err("invalid issuer URL")?;

        let client = Client::discover(
            client_id.clone(),
            None, // client_secret
            None, // redirect_uri
            issuer,
        )
        .await
        .into_diagnostic()
        .wrap_err("failed to discover OIDC configuration")?;

        Ok(Self {
            client,
            audience,
        })
    }

    pub async fn validate_token(&self, token: &str) -> Result<OidcClaims> {
        // Decode and validate token
        let mut token_data = Token::from_str(token)
            .into_diagnostic()
            .wrap_err("invalid token format")?;

        // Validate token with OIDC provider
        let user_info = self.client
            .request_userinfo(&token_data)
            .await
            .into_diagnostic()
            .wrap_err("failed to validate token")?;

        // Verify audience
        let claims = token_data.id_token
            .ok_or_else(|| miette::miette!("missing id_token"))?;

        if let Some(aud) = &claims.aud {
            if !aud.contains(&self.audience) {
                return Err(miette::miette!("invalid audience"));
            }
        }

        Ok(OidcClaims {
            subject: claims.sub,
            display_name: user_info.name.unwrap_or_else(|| claims.sub.clone()),
            email: user_info.email,
        })
    }
}
*/
