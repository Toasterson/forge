use axum::extract::{Host, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{AppState, Result};
use crate::{prisma, Error};

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<OpenIdConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<OpenIdConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct OpenIdConfig {
    pub client_id: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/login_info",
    responses (
        (status = 200, description = "Successfully got the Publishers", body = AuthConfig),
        (status = 404, description = "No such domain on this forge", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
pub async fn login_info(
    State(state): State<AppState>,
    Host(host): Host,
) -> Result<Json<AuthConfig>> {
    let domain = state
        .prisma
        .lock()
        .await
        .domain()
        .find_unique(prisma::domain::UniqueWhereParam::DnsNameEquals(host))
        .exec()
        .await?;

    if let Some(domain) = domain {
        let auth_config: AuthConfig = serde_json::from_value(domain.authconf)?;

        Ok(Json(auth_config))
    } else {
        Err(Error::NoDomainFound)
    }
}
