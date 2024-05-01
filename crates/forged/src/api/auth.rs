use axum::{async_trait, RequestPartsExt};
use axum::extract::{FromRef, FromRequestParts, Host};
use axum::http::request::Parts;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::Bearer;
use axum_extra::TypedHeader;
use pasetors::claims::ClaimsValidationRules;
use pasetors::keys::AsymmetricPublicKey;
use pasetors::Public;
use pasetors::token::{TrustedToken, UntrustedToken};
use pasetors::version4::V4;

use crate::{AppState, Error, prisma};

pub struct Authentication {
    pub token: TrustedToken
}

#[async_trait]
impl <S> FromRequestParts<S> for Authentication
    where
        AppState: FromRef<S>,
        S: Send + Sync,
{
    type Rejection = crate::Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let TypedHeader(authorization) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| Error::Unauthorized)?;

        let Host(host) = parts
            .extract::<Host>()
            .await
            .map_err(|_| Error::Unauthorized)?;

        // this throws an error
        let state = parts
            .extract_with_state::<AppState, _>(state)
            .await
            .map_err(|_| Error::Unauthorized)?;

        let domain = state
            .prisma
            .lock()
            .await
            .domain()
            .find_unique(prisma::domain::UniqueWhereParam::DnsNameEquals(host))
            .exec()
            .await?.ok_or(Error::Unauthorized)?;
        
        let public_key = AsymmetricPublicKey::<V4>::try_from(domain.public_key.as_str())?;
        let untrusted_token = UntrustedToken::<Public, V4>::try_from(authorization.token())?;
        let validation_rules = ClaimsValidationRules::new();
        let token = pasetors::public::verify(
            &public_key,
            &untrusted_token,
            &validation_rules,
            None,
            None,
        )?;
        
        Ok(Self{
            token,
        })
    }
}