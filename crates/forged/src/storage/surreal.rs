use miette::{Context, IntoDiagnostic};
use serde::Serialize;
use surrealdb::engine::any::{connect, Any};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use crate::settings::SurrealConfig;
use crate::types::{ComponentId, GateId};
use crate::{component::ComponentRecord, gate::GateRecord};

pub type Db = Surreal<Any>;

pub async fn connect_from_config(cfg: &SurrealConfig) -> miette::Result<Db> {
    // Determine engine URI
    let mode = cfg.mode.as_deref().unwrap_or("embedded");
    let ns = cfg.namespace.as_deref().unwrap_or("forged");
    let db = cfg.database.as_deref().unwrap_or("default");
    let uri = match mode {
        "clustered" => {
            let endpoint = cfg.endpoint.as_deref().unwrap_or("ws://127.0.0.1:8000");
            endpoint.to_string()
        }
        _ => {
            // embedded
            let path = cfg.path.as_deref().unwrap_or("./data/surreal");
            format!("rocksdb:{}", path)
        }
    };

    let dbh = connect(uri.as_str())
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("connect surrealdb at {uri}"))?;

    if mode == "clustered" {
        if let (Some(user), Some(pass)) = (cfg.username.clone(), cfg.password.clone()) {
            dbh.signin(Root {
                username: &user,
                password: &pass,
            })
            .await
            .into_diagnostic()
            .wrap_err("surreal sign-in failed")?;
        }
    }

    dbh.use_ns(ns)
        .use_db(db)
        .await
        .into_diagnostic()
        .wrap_err("select surreal ns/db")?;

    Ok(dbh)
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ServerSettingsRec {
    pub private_key_ssh: String,
    pub public_key_ssh: String,
    pub created_at: u64,
}

pub async fn get_server_settings(db: &Db) -> miette::Result<Option<ServerSettingsRec>> {
    let key: surrealdb::sql::Thing = ("settings", "server_keys").into();
    let res: Option<ServerSettingsRec> = db
        .select(key)
        .await
        .into_diagnostic()
        .wrap_err("select server settings")?;
    Ok(res)
}

pub async fn put_server_settings(db: &Db, s: &ServerSettingsRec) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("settings", "server_keys").into();
    let _res: Option<ServerSettingsRec> = db
        .update(key)
        .content(s)
        .await
        .into_diagnostic()
        .wrap_err("upsert server settings")?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct PendingRegistrationRec {
    #[serde(rename = "id")]
    pub id: String,
    pub actor_id: String,
    pub actor_kind: i32,
    pub expires_at: u64,
    pub envelope: Vec<u8>,
}

pub async fn upsert_pending_registration(
    db: &Db,
    rec: &PendingRegistrationRec,
) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("pending_registrations", rec.id.as_str()).into();
    let _res: Option<PendingRegistrationRec> = db
        .update(key)
        .content(rec)
        .await
        .into_diagnostic()
        .wrap_err("upsert pending registration")?;
    Ok(())
}

pub async fn get_pending_registration(
    db: &Db,
    id: &str,
) -> miette::Result<Option<PendingRegistrationRec>> {
    let key: surrealdb::sql::Thing = ("pending_registrations", id).into();
    let res: Option<PendingRegistrationRec> = db
        .select(key)
        .await
        .into_diagnostic()
        .wrap_err("get pending registration")?;
    Ok(res)
}

pub async fn delete_pending_registration(db: &Db, id: &str) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("pending_registrations", id).into();
    let _: Option<PendingRegistrationRec> = db
        .delete(key)
        .await
        .into_diagnostic()
        .wrap_err("delete pending registration")?;
    Ok(())
}

// Gate and Component storage helpers
pub async fn put_gate(db: &Db, gate: &GateRecord) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("gates", gate.id.0.as_str()).into();
    let _: Option<GateRecord> = db
        .update(key)
        .content(gate)
        .await
        .into_diagnostic()
        .wrap_err("upsert gate record")?;
    Ok(())
}

pub async fn get_gate(db: &Db, id: &GateId) -> miette::Result<Option<GateRecord>> {
    let key: surrealdb::sql::Thing = ("gates", id.0.as_str()).into();
    let res: Option<GateRecord> = db
        .select(key)
        .await
        .into_diagnostic()
        .wrap_err("get gate record")?;
    Ok(res)
}

pub async fn delete_gate(db: &Db, id: &GateId) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("gates", id.0.as_str()).into();
    let _: Option<GateRecord> = db
        .delete(key)
        .await
        .into_diagnostic()
        .wrap_err("delete gate record")?;
    Ok(())
}

pub async fn list_gates(db: &Db) -> miette::Result<Vec<GateRecord>> {
    let res: Vec<GateRecord> = db
        .select("gates")
        .await
        .into_diagnostic()
        .wrap_err("list gate records")?;
    Ok(res)
}

pub async fn put_component(db: &Db, rec: &ComponentRecord) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("components", rec.id.0.as_str()).into();
    let _: Option<ComponentRecord> = db
        .update(key)
        .content(rec)
        .await
        .into_diagnostic()
        .wrap_err("upsert component record")?;
    Ok(())
}

pub async fn get_component(db: &Db, id: &ComponentId) -> miette::Result<Option<ComponentRecord>> {
    let key: surrealdb::sql::Thing = ("components", id.0.as_str()).into();
    let res: Option<ComponentRecord> = db
        .select(key)
        .await
        .into_diagnostic()
        .wrap_err("get component record")?;
    Ok(res)
}

pub async fn delete_component(db: &Db, id: &ComponentId) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("components", id.0.as_str()).into();
    let _: Option<ComponentRecord> = db
        .delete(key)
        .await
        .into_diagnostic()
        .wrap_err("delete component record")?;
    Ok(())
}

pub async fn list_components(db: &Db) -> miette::Result<Vec<ComponentRecord>> {
    let res: Vec<ComponentRecord> = db
        .select("components")
        .await
        .into_diagnostic()
        .wrap_err("list component records")?;
    Ok(res)
}
