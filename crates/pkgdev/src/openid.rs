use std::ops::Add;
use std::str::FromStr;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use forge::AuthConfig;
use crate::forge::{Result, Error, LoginProvider};

const GITHUB_DEVICE_AUTH_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_DEVICE_OAUTH_POLL_URL: &str = "https://github.com/login/oauth/access_token";

#[derive(Serialize)]
struct GitHubDeviceCodeRequest {
    client_id: String,
    scope: String,
}

#[derive(Deserialize)]
struct GitHubDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[allow(dead_code)]
    expires_in: u64,
    interval: u64,
}

#[derive(Serialize)]
struct GitHubDeviceAccessTokenRequest {
    client_id: String,
    device_code: String,
    grant_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitHubAccessToken {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitHubDeviceCodeError {
    error: GitHubDeviceCodeErrorType,
    error_description: String,
    error_uri: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GitHubDeviceCodeErrorType {
    AuthorizationPending,
    SlowDown,
    ExpiredToken,
    UnsupportedGrantType,
    IncorrectClientCredentials,
    IncorrectDeviceCode,
    AccessDenied,
    DeviceFlowDisabled,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum GitHubDeviceResponse {
    Success(GitHubAccessToken),
    Error(GitHubDeviceCodeError),
}

#[derive(Serialize, Deserialize, Clone)]
pub enum OauthToken {
    GitHub(GitHubAccessToken)
}

pub fn login_to_provider(login_provider: &LoginProvider, info: &AuthConfig) -> Result<OauthToken> {
    match login_provider {
        LoginProvider::Github => {
            let gh_info = info.github.clone().ok_or(Error::OAuthProviderNotConnected)?;
            let resp = reqwest::blocking::Client::new()
                .post(GITHUB_DEVICE_AUTH_CODE_URL)
                .header(reqwest::header::ACCEPT, "application/json")
                .json(&GitHubDeviceCodeRequest{
                    client_id: gh_info.client_id.clone(),
                    scope: String::from("read:user,read:project,read:gpg_key"),
                })
                .send()?;
            let gh_device_code_resp: GitHubDeviceCodeResponse = resp.json()?;
            let mut sleep_duration = Duration::from_secs(gh_device_code_resp.interval + 1);
            println!("To Login with GitHub visit: {} and enter the code {} ", gh_device_code_resp.verification_uri, gh_device_code_resp.user_code);
            let oauth_token: GitHubAccessToken;
            'poll_loop: loop {
                std::thread::sleep(sleep_duration);
                let resp = reqwest::blocking::Client::new()
                    .post(GITHUB_DEVICE_OAUTH_POLL_URL)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .json(&GitHubDeviceAccessTokenRequest{
                        client_id: gh_info.client_id.clone(),
                        device_code: gh_device_code_resp.device_code.clone(),
                        grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                    })
                    .send()?;
                if resp.status().is_success() {
                    let oauth_resp: GitHubDeviceResponse = resp.json()?;
                    match oauth_resp {
                        GitHubDeviceResponse::Success(token) => {
                            oauth_token = token;
                            break 'poll_loop;
                        }
                        GitHubDeviceResponse::Error(err) => {
                            match err.error {
                                GitHubDeviceCodeErrorType::AuthorizationPending => {}
                                GitHubDeviceCodeErrorType::SlowDown => {
                                    sleep_duration = sleep_duration.add(Duration::from_secs(6))
                                }
                                GitHubDeviceCodeErrorType::ExpiredToken => {
                                    println!("Login expired please try again");
                                    return Err(Error::LoginAborted);
                                }
                                GitHubDeviceCodeErrorType::AccessDenied => {
                                    return Err(Error::LoginAborted);
                                }
                                _ => {
                                    println!("An error occured during the login process:");
                                    println!("{}", err.error_description);
                                    println!("visit: {}  for more details", err.error_uri);
                                    return Err(Error::LoginAborted);
                                }
                            }
                        }
                    }
                } else {
                    println!("DEBUG: Response from server: {}", resp.text()?);
                }
            }
            
            Ok(OauthToken::GitHub(oauth_token))
        }
        LoginProvider::Gitlab => {
            todo!();
        }
    }
}