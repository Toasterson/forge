use crate::api::forged::api::v1 as api;
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

    fn client(&self) -> api::component_service_client::ComponentServiceClient<Channel> {
        api::component_service_client::ComponentServiceClient::new(self.channel.clone())
    }

    pub async fn create_component(
        &self,
        component: api::Component,
        token: &str,
    ) -> Result<api::Component> {
        let req = api::CreateComponentRequest {
            component: Some(component),
        };
        let mut c = self.client();
        let resp = c
            .create_component(authenticated_request(req, token))
            .await?;
        Ok(resp.into_inner().component.unwrap_or_default())
    }

    pub async fn list_components(&self, token: &str) -> Result<Vec<api::Component>> {
        let mut c = self.client();
        let resp = c
            .list_components(authenticated_request(
                api::ListComponentsRequest {
                    page_size: 0,
                    page_token: String::new(),
                },
                token,
            ))
            .await?;
        Ok(resp.into_inner().components)
    }

    pub async fn get_component(&self, id: &str, token: &str) -> Result<Option<api::Component>> {
        let mut c = self.client();
        let resp = c
            .get_component(authenticated_request(
                api::GetComponentRequest { id: id.to_string() },
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
