use crate::Result;
use crate::{prisma, Error, SharedState};
use axum::extract::{Host, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

pub fn get_router() -> Router<SharedState> {
    Router::new().route("/login_info", get(login_info))
}

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    provider: OauthProvider,
}

#[derive(Deserialize, Serialize)]
pub enum OauthProvider {
    GitHub,
    Gitlab,
}

async fn login_info(
    State(state): State<SharedState>,
    Host(host): Host,
) -> Result<Json<forge::AuthConfig>> {
    let domain = state
        .lock()
        .await
        .prisma
        .domain()
        .find_unique(prisma::domain::UniqueWhereParam::DnsNameEquals(host))
        .exec()
        .await?;

    if let Some(domain) = domain {
        let auth_config: forge::AuthConfig = serde_json::from_value(domain.authconf)?;

        Ok(Json(auth_config))
    } else {
        Err(Error::NoDomainFound)
    }
}
