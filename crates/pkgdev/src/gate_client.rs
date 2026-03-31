use crate::api::forged::api::v2 as api_v2;
use crate::auth::authenticated_request;
use miette::Diagnostic;
use thiserror::Error;
use tonic::transport::Channel;

#[derive(Error, Debug, Diagnostic)]
#[diagnostic(code(ips::gate_error), help("check server address and parameters"))]
pub enum GateClientError {
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),

    #[error(transparent)]
    Status(#[from] tonic::Status),

    #[error("invalid server url: {0}")]
    InvalidServerUrl(String),
}

pub type Result<T, E = GateClientError> = miette::Result<T, E>;

#[derive(Clone)]
pub struct GateClient {
    #[allow(dead_code)]
    server: String,
    channel: Channel,
}

impl GateClient {
    pub async fn connect<S: Into<String>>(server: S) -> Result<Self> {
        let server = server.into();
        if !server.starts_with("http://") && !server.starts_with("https://") {
            return Err(GateClientError::InvalidServerUrl(server));
        }
        let endpoint = Channel::from_shared(server.clone())
            .map_err(|_| GateClientError::InvalidServerUrl(server.clone()))?;
        let channel = endpoint.connect().await?;
        Ok(Self { server, channel })
    }

    fn client(&self) -> api_v2::gate_service_client::GateServiceClient<Channel> {
        api_v2::gate_service_client::GateServiceClient::new(self.channel.clone())
    }

    fn actor_ref(actor_id: &str) -> Option<api_v2::ActorRef> {
        Some(api_v2::ActorRef {
            id: actor_id.to_string(),
            kind: "user".to_string(),
        })
    }

    pub async fn list_gates(&self, actor_id: &str, token: &str) -> Result<Vec<api_v2::GateInfo>> {
        let mut c = self.client();
        let resp = c
            .list_gates(authenticated_request(
                api_v2::ListGatesRequest {
                    actor: Self::actor_ref(actor_id),
                    owner_id: None,
                    page_size: 0,
                    page_token: String::new(),
                },
                token,
            ))
            .await?;
        Ok(resp.into_inner().gates)
    }

    pub async fn get_gate(
        &self,
        actor_id: &str,
        gate_id: &str,
        token: &str,
    ) -> Result<Option<api_v2::GateInfo>> {
        let mut c = self.client();
        let resp = c
            .get_gate(authenticated_request(
                api_v2::GetGateRequest {
                    actor: Self::actor_ref(actor_id),
                    gate_id: Some(api_v2::GateId {
                        id: gate_id.to_string(),
                    }),
                },
                token,
            ))
            .await;
        match resp {
            Ok(r) => Ok(r.into_inner().gate),
            Err(status) => {
                if status.code() == tonic::Code::NotFound {
                    Ok(None)
                } else {
                    Err(status.into())
                }
            }
        }
    }

    pub async fn list_members(
        &self,
        actor_id: &str,
        gate_id: &str,
        token: &str,
    ) -> Result<Vec<api_v2::GateMemberInfo>> {
        let mut c = self.client();
        let resp = c
            .list_members(authenticated_request(
                api_v2::ListMembersRequest {
                    actor: Self::actor_ref(actor_id),
                    gate_id: Some(api_v2::GateId {
                        id: gate_id.to_string(),
                    }),
                    page_size: 0,
                    page_token: String::new(),
                },
                token,
            ))
            .await?;
        Ok(resp.into_inner().members)
    }

    pub async fn list_components(
        &self,
        actor_id: &str,
        gate_id: &str,
        token: &str,
    ) -> Result<Vec<api_v2::ComponentInfo>> {
        let mut c = self.client();
        let resp = c
            .list_components(authenticated_request(
                api_v2::ListComponentsRequest {
                    actor: Self::actor_ref(actor_id),
                    gate_id: Some(api_v2::GateId {
                        id: gate_id.to_string(),
                    }),
                    page_size: 0,
                    page_token: String::new(),
                },
                token,
            ))
            .await?;
        Ok(resp.into_inner().components)
    }
}
