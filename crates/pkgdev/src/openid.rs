use std::ops::Add;
use std::time::Duration;

use either::Either;
use octocrab::auth::{Continue, OAuth};
use reqwest::header::ACCEPT;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use forge::AuthConfig;

use crate::forge::{Error, LoginProvider, Result};

#[derive(Clone, Deserialize, Serialize)]
pub struct OAuthConfig {
    pub access_token: String,
    pub token_type: String,
    pub scope: Vec<String>,
    pub expires_in: Option<usize>,
    pub refresh_token: Option<String>,
    pub refresh_token_expires_in: Option<usize>,
}

impl From<OAuth> for OAuthConfig {
    fn from(value: OAuth) -> Self {
        Self {
            access_token: value.access_token.expose_secret().clone(),
            token_type: value.token_type,
            scope: value.scope,
            expires_in: value.expires_in,
            refresh_token: value.refresh_token.map(|s| s.expose_secret().clone()),
            refresh_token_expires_in: value.refresh_token_expires_in,
        }
    }
}

#[derive(Deserialize)]
pub struct OAuthWire {
    access_token: String,
    token_type: String,
    scope: String,
    expires_in: Option<usize>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<usize>,
}

impl From<OAuthWire> for OAuthConfig {
    fn from(value: OAuthWire) -> Self {
        OAuthConfig {
            access_token: value.access_token,
            token_type: value.token_type,
            scope: value.scope.split(',').map(ToString::to_string).collect(),
            expires_in: value.expires_in,
            refresh_token: value.refresh_token,
            refresh_token_expires_in: value.refresh_token_expires_in,
        }
    }
}

impl Into<OAuth> for OAuthConfig {
    fn into(self) -> OAuth {
        OAuth {
            access_token: self.access_token.into(),
            token_type: self.token_type,
            scope: self.scope,
            expires_in: self.expires_in,
            refresh_token: self.refresh_token.map(|s| s.into()),
            refresh_token_expires_in: self.refresh_token_expires_in,
        }
    }
}

pub async fn login_to_provider(login_provider: &LoginProvider, info: &AuthConfig) -> Result<OAuth> {
    match login_provider {
        LoginProvider::Github => {
            let gh_info = info
                .github
                .clone()
                .ok_or(Error::OAuthProviderNotConnected)?;
            let client_id: SecretString = gh_info.client_id.into();
            let crabby = octocrab::Octocrab::builder()
                .base_uri("https://github.com")?
                .add_header(ACCEPT, "application/json".to_string())
                .build()?;

            let device_flow_resp = crabby
                .authenticate_as_device(&client_id, ["read:user", "read:project", "read:gpg_key"])
                .await?;

            let mut sleep_duration = Duration::from_secs(device_flow_resp.interval + 1);
            println!(
                "To Login with GitHub visit: {} and enter the code {} ",
                device_flow_resp.verification_uri, device_flow_resp.user_code
            );

            loop {
                tokio::time::sleep(sleep_duration).await;
                let poll_resp = device_flow_resp.poll_once(&crabby, &client_id).await?;
                match poll_resp {
                    Either::Left(l) => {
                        return Ok(l);
                    }
                    Either::Right(r) => match r {
                        Continue::SlowDown => {
                            sleep_duration = sleep_duration.add(Duration::from_secs(6))
                        }
                        Continue::AuthorizationPending => {}
                    },
                }
            }
        }
        LoginProvider::Gitlab => {
            todo!();
        }
    }
}
