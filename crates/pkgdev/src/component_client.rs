use crate::api::forged::api::v2 as api_v2;
use crate::auth::authenticated_request;
use miette::Diagnostic;
use thiserror::Error;
use tonic::transport::Channel;

#[derive(Error, Debug, Diagnostic)]
#[diagnostic(
    code(ips::component_error),
    help("check server address and parameters")
)]
pub enum ComponentClientError {
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),

    #[error(transparent)]
    Status(#[from] tonic::Status),

    #[error("invalid server url: {0}")]
    InvalidServerUrl(String),
}

pub type Result<T, E = ComponentClientError> = miette::Result<T, E>;

#[derive(Clone)]
pub struct ComponentClient {
    #[allow(dead_code)]
    server: String,
    channel: Channel,
}

impl ComponentClient {
    pub async fn connect<S: Into<String>>(server: S) -> Result<Self> {
        let server = server.into();
        if !server.starts_with("http://") && !server.starts_with("https://") {
            return Err(ComponentClientError::InvalidServerUrl(server));
        }
        let endpoint = Channel::from_shared(server.clone())
            .map_err(|_| ComponentClientError::InvalidServerUrl(server.clone()))?;
        let channel = endpoint.connect().await?;
        Ok(Self { server, channel })
    }

    fn actor_ref(actor_id: &str) -> Option<api_v2::ActorRef> {
        Some(api_v2::ActorRef {
            id: actor_id.to_string(),
            kind: "user".to_string(),
        })
    }

    pub async fn get_component(
        &self,
        actor_id: &str,
        component_id: &str,
        token: &str,
    ) -> Result<Option<api_v2::ComponentInfo>> {
        let mut client =
            api_v2::component_service_client::ComponentServiceClient::new(self.channel.clone());
        let resp = client
            .get_component(authenticated_request(
                api_v2::GetComponentRequest {
                    actor: Self::actor_ref(actor_id),
                    component_id: Some(api_v2::ComponentId {
                        id: component_id.to_string(),
                    }),
                },
                token,
            ))
            .await;
        match resp {
            Ok(r) => Ok(r.into_inner().component),
            Err(status) => {
                if status.code() == tonic::Code::NotFound {
                    Ok(None)
                } else {
                    Err(status.into())
                }
            }
        }
    }
}
