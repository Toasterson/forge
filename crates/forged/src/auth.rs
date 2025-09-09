use crate::rbac::Permission;
use crate::types::ActorKind;
use jsonwebtoken as jwt;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // subject (actor id)
    pub actor_kind: ActorKind,
    pub roles: Vec<String>, // role names
    pub permissions: Vec<Permission>,
    pub iat: u64,    // issued at (seconds)
    pub exp: u64,    // expiration (seconds)
    pub jti: String, // token id
}

impl Claims {
    pub fn new(
        sub: String,
        actor_kind: ActorKind,
        roles: Vec<String>,
        permissions: Vec<Permission>,
        ttl: Duration,
    ) -> Self {
        let now = now_sec();
        Self {
            sub,
            actor_kind,
            roles,
            permissions,
            iat: now,
            exp: now + ttl.as_secs(),
            jti: uuid::Uuid::new_v4().to_string(),
        }
    }
}

#[derive(Clone)]
pub enum JwtKey {
    Hs256(String),
}

impl JwtKey {
    fn encoding(&self) -> EncodingKey {
        match self {
            JwtKey::Hs256(secret) => EncodingKey::from_secret(secret.as_bytes()),
        }
    }
    fn decoding(&self) -> DecodingKey {
        match self {
            JwtKey::Hs256(secret) => DecodingKey::from_secret(secret.as_bytes()),
        }
    }
}

pub fn encode_access_token(claims: &Claims, key: &JwtKey) -> Result<String, jwt::errors::Error> {
    jwt::encode(&Header::default(), claims, &key.encoding())
}

pub fn decode_access_token(token: &str, key: &JwtKey) -> Result<Claims, jwt::errors::Error> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    let data = jwt::decode::<Claims>(token, &key.decoding(), &validation)?;
    Ok(data.claims)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub subject: String,
    pub refresh_token: String,
    pub created_at: u64,
    pub expires_at: u64,
}

impl Session {
    pub fn new(subject: String, ttl: Duration) -> Self {
        let now = now_sec();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            subject,
            refresh_token: uuid::Uuid::new_v4().to_string(),
            created_at: now,
            expires_at: now + ttl.as_secs(),
        }
    }
}

pub trait SessionStore: Send + Sync {
    fn put(&self, session: &Session) -> anyhow::Result<()>;
    fn get(&self, id: &str) -> anyhow::Result<Option<Session>>;
    fn revoke(&self, id: &str) -> anyhow::Result<()>;
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::Permission;

    #[test]
    fn jwt_roundtrip() {
        let claims = Claims::new(
            "user-1".to_string(),
            ActorKind::User,
            vec!["owner".to_string()],
            vec![Permission::GateRead],
            Duration::from_secs(60),
        );
        let key = JwtKey::Hs256("secret".to_string());
        let token = encode_access_token(&claims, &key).unwrap();
        let decoded = decode_access_token(&token, &key).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.actor_kind as u8, claims.actor_kind as u8);
    }
}
