use crate::api::forged::api::v1 as api;
use miette::Diagnostic;
use thiserror::Error;
use tonic::transport::Channel;
use tonic::Request;

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

    fn client(&self) -> api::gate_service_client::GateServiceClient<Channel> {
        api::gate_service_client::GateServiceClient::new(self.channel.clone())
    }

    pub async fn create_gate(&self, gate: api::Gate) -> Result<api::Gate> {
        let req = api::CreateGateRequest { gate: Some(gate) };
        let mut c = self.client();
        let resp = c.create_gate(Request::new(req)).await?;
        Ok(resp.into_inner().gate.unwrap_or_default())
    }

    pub async fn list_gates(&self) -> Result<Vec<api::Gate>> {
        let mut c = self.client();
        let resp = c
            .list_gates(Request::new(api::ListGatesRequest {
                page_size: 0,
                page_token: String::new(),
            }))
            .await?;
        Ok(resp.into_inner().gates)
    }

    pub async fn get_gate(&self, id: &str) -> Result<Option<api::Gate>> {
        let mut c = self.client();
        let resp = c
            .get_gate(Request::new(api::GetGateRequest { id: id.to_string() }))
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
}
