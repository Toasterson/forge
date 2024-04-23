use std::ops::Add;

use axum::{Json, Router};
use axum::extract::{Host, State};
use axum::routing::post;
use chrono::TimeDelta;
use octorust::auth::Credentials;
use rusty_paseto::prelude::*;

use forge::{ActorConnectRequest, ActorConnectResponse, ActorSSHKeyFingerprint};

use crate::{Error, prisma, Result, SharedState};
use crate::prisma::KeyType;

const SSH_RSA: &str = "ssh-rsa";
const SSH_ED25519: &str = "ssh-ed25519";
const SSH_ECDSA_256: &str = "ecdsa-sha2-nistp256";
const SSH_ECDSA_384: &str = "ecdsa-sha2-nistp384";
const SSH_ECDSA_521: &str = "ecdsa-sha2-nistp521";

pub fn get_router() -> Router<SharedState> {
    Router::new().route("/connect", post(actor_connect))
}

async fn actor_connect(
    State(state): State<SharedState>,
    Host(host): Host,
    Json(request): Json<ActorConnectRequest>,
) -> Result<Json<ActorConnectResponse>> {
    match request {
        ActorConnectRequest::GitHub { handle, token, display_name } => {
            let gh_client = octorust::Client::new(
                format!("package forge {host}"),
                Credentials::Token(token)
            )?;

            let domain_data = state.lock().await
                .prisma.domain().find_unique(prisma::domain::UniqueWhereParam::DnsNameEquals(
                host.clone()
            )).exec().await?.ok_or(Error::NoDomainFound)?;
            let key =
                PasetoAsymmetricPrivateKey::<V4, Public>::from(domain_data.key.as_slice());

            let ssh_keys = gh_client.users()
                .list_all_public_ssh_keys_for_authenticated().await?;
            let user_details = gh_client.users()
                .get_authenticated_private_user().await?;

            // Try to find the user by the handle and domain
            if let Some(existing_actor) = state.lock().await
                .prisma.actor()
                .find_unique(prisma::actor::UniqueWhereParam::HandleEquals(
                    handle.clone())
                ).with(prisma::actor::keys::fetch(vec![]))
                .exec().await? {
                if !existing_actor.remote_handles.contains(&user_details.body.name) {
                    return Err(Error::UnauthorizedToClaimHandle);
                }

                let (access_token, refresh_token) =
                    make_login_token(&handle, &domain_data.dns_name, &existing_actor.display_name ,&key)?;

                let resp_ssh_keys = ssh_keys.body.iter()
                    .filter_map(filter_map_ssh_keys_to_fingerprint).collect();

                let db_keys = ssh_keys.body.iter().filter_map(|k|
                    ssh_keys_to_db_format(&existing_actor.id, k))
                    .collect::<Vec<(String, String, String, Vec<prisma::key::SetParam>)>>();

                if let Some(keys) = existing_actor.keys {
                    // If we have one of the existing keys not in the new keys add it to the delete list
                    let delete_list = keys.iter().filter(|k| {
                        db_keys.iter().filter_map(|(_,name, _, _)| if name == &k.name {Some(name.clone())} else {None}).collect::<Vec<String>>().len() == 0
                    })
                        .map(|k| prisma::key::id::equals(k.id.clone()))
                        .collect::<Vec<prisma::key::WhereParam>>();
                    state.lock().await.prisma.key().delete_many(delete_list).exec().await?;
                    
                    let create_list = db_keys.into_iter().filter(|(_, name, _, _)| {
                        keys.iter().filter_map(|k| if &k.name == name {Some(k.name.clone())} else {None}).collect::<Vec<String>>().len() == 0
                    }).collect::<Vec<(String, String, String, Vec<prisma::key::SetParam>)>>();
                    state.lock().await.prisma.key().create_many(create_list).exec().await?;
                    
                } else {
                    state.lock().await.prisma.key().create_many(db_keys).exec().await?;
                }

                Ok(Json(ActorConnectResponse{
                    access_token,
                    refresh_token,
                    ssh_keys: resp_ssh_keys,
                    handle: existing_actor.handle
                }))
            } else {
                let actor = state.lock().await.prisma.actor().create(
                    display_name.unwrap_or(handle.clone()),
                    handle,
                    prisma::domain::UniqueWhereParam::DnsNameEquals(domain_data.dns_name.clone()),
                    vec![],
                ).exec().await?;

                let resp_ssh_keys = ssh_keys.body.iter()
                    .filter_map(filter_map_ssh_keys_to_fingerprint).collect();

                let db_keys = ssh_keys.body.iter().filter_map(|k|
                    ssh_keys_to_db_format(&actor.id, k))
                    .collect::<Vec<(String, String, String, Vec<prisma::key::SetParam>)>>();

                state.lock().await.prisma.key().create_many(db_keys).exec().await?;

                let (access_token, refresh_token) =
                    make_login_token(&actor.handle, &domain_data.dns_name, &actor.display_name ,&key)?;

                Ok(Json(ActorConnectResponse{
                    access_token,
                    refresh_token,
                    ssh_keys: resp_ssh_keys,
                    handle: actor.handle
                }))
            }
        }
        ActorConnectRequest::GitLab { .. } => {
            todo!()
        }
    }
}

fn make_login_token(handle: &str, domain: &str, display_name: &str, key: &PasetoAsymmetricPrivateKey<V4, Public>) -> Result<(String, String)> {
    let now = chrono::Utc::now();
    let access_expiration = now.add(TimeDelta::hours(8));
    let refresh_expiration = now.add(TimeDelta::days(90));

    let access_token = PasetoBuilder::<V4, Public>::default()
        .set_claim(NotBeforeClaim::try_from(now.format("%+").to_string())?)
        .set_claim(SubjectClaim::from(handle))
        .set_claim(IssuerClaim::from(domain))
        .set_claim(CustomClaim::try_from(("displayName", display_name))?)
        .set_claim(ExpirationClaim::try_from(access_expiration.format("%+").to_string())?)
        .build(key)?;

    let refresh_token = PasetoBuilder::<V4, Public>::default()
        .set_claim(NotBeforeClaim::try_from(now.format("%+").to_string())?)
        .set_claim(SubjectClaim::from(handle))
        .set_claim(IssuerClaim::from(domain))
        .set_claim(ExpirationClaim::try_from(refresh_expiration.format("%+").to_string())?)
        .build(key)?;

    Ok((access_token, refresh_token))
}

fn ssh_keys_to_db_format(actor_id: &str, s: &octorust::types::Key) -> Option<(String, String, String, Vec<prisma::key::SetParam>)> {
    if let Some(key) = openssh_keys::PublicKey::parse(&s.key).ok() {
        match key.keytype() {
            SSH_RSA => Some((actor_id.to_string(), s.title.clone(), s.key.clone(), vec![prisma::key::SetParam::SetKeyType(KeyType::Rsa)])),
            SSH_ED25519 => Some((actor_id.to_string(), s.title.clone(), s.key.clone(), vec![prisma::key::SetParam::SetKeyType(KeyType::Ed25519)])),
            SSH_ECDSA_256 | SSH_ECDSA_384 | SSH_ECDSA_521 => Some((actor_id.to_string(), s.title.clone(), s.key.clone(), vec![prisma::key::SetParam::SetKeyType(KeyType::Ecdsa)])),
            &_ => None,
        }
    } else {
        None
    }
}

fn filter_map_ssh_keys_to_fingerprint(s: &octorust::types::Key) -> Option<ActorSSHKeyFingerprint> {
    if let Some(key) = openssh_keys::PublicKey::parse(&s.key).ok() {
        match key.keytype() {
            SSH_RSA => Some(ActorSSHKeyFingerprint::Rsa(key.fingerprint())),
            SSH_ED25519 => Some(ActorSSHKeyFingerprint::Ed25519(key.fingerprint())),
            SSH_ECDSA_256 | SSH_ECDSA_384 | SSH_ECDSA_521 => Some(ActorSSHKeyFingerprint::ECDSA(key.fingerprint())),
            _ => None,
        }
    } else {
        None
    }
}