use miette::{Context, IntoDiagnostic};
use mongodb::{options::ClientOptions, Client};

use crate::component::ComponentRecord;
use crate::gate::GateRecord;
use crate::types::{ComponentId, GateId};

pub mod json;

/// Repository trait for storing and retrieving Gate records.
pub trait GateStore: Send + Sync {
    fn put_gate(&self, gate: &GateRecord) -> miette::Result<()>;
    fn get_gate(&self, id: &GateId) -> miette::Result<Option<GateRecord>>;
    fn delete_gate(&self, id: &GateId) -> miette::Result<()>;
}

/// Repository trait for storing and retrieving Components.
pub trait ComponentStore: Send + Sync {
    fn put_component(&self, id: &ComponentId, component: &ComponentRecord) -> miette::Result<()>;
    fn get_component(&self, id: &ComponentId) -> miette::Result<Option<ComponentRecord>>;
    fn delete_component(&self, id: &ComponentId) -> miette::Result<()>;
}

pub async fn connect(uri: &str) -> miette::Result<Client> {
    let opts = ClientOptions::parse(uri)
        .await
        .into_diagnostic()
        .wrap_err("parse mongodb uri")?;
    let client = Client::with_options(opts)
        .into_diagnostic()
        .wrap_err("create mongodb client")?;
    Ok(client)
}
