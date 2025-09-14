use miette::{Context, IntoDiagnostic};
use serde::{Deserialize, Serialize};
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
    // Surreal stores its own record Thing as `id`. We keep a local id for keying,
    // but do not serialize it into the stored content and ignore it on load.
    #[serde(rename = "id", skip_serializing, default, skip_deserializing)]
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
// Internal row shape returned by Surreal when selecting from 'gates' includes an 'id' Thing.
#[derive(Deserialize)]
struct GateRow {
    id: surrealdb::sql::Thing,
    base: gate::Gate,
    owner: crate::types::ActorRef,
    members: Vec<crate::gate::GateMember>,
    created_at: u64,
    updated_at: u64,
    metadata: Option<serde_json::Value>,
}

fn thing_to_gate_id(t: &surrealdb::sql::Thing) -> crate::types::GateId {
    use surrealdb::sql::Id;
    let s = match &t.id {
        Id::String(s) => s.clone(),
        other => other.to_string(),
    };
    crate::types::GateId(s)
}

fn row_to_gate_record(r: GateRow) -> crate::gate::GateRecord {
    crate::gate::GateRecord {
        id: thing_to_gate_id(&r.id),
        base: r.base,
        owner: r.owner,
        members: r.members,
        created_at: r.created_at,
        updated_at: r.updated_at,
        metadata: r.metadata,
    }
}
pub async fn put_gate(db: &Db, gate: &GateRecord) -> miette::Result<()> {
    // Avoid including the `id` field in the content payload to Surreal, as we are
    // already addressing the record by key. Including `id` causes Surreal to error
    // with: "Found s'<id>' for the id field, but a specific record has been specified".
    #[derive(Serialize)]
    struct GateRecordContent<'a> {
        pub base: &'a gate::Gate,
        pub owner: &'a crate::types::ActorRef,
        pub members: &'a Vec<crate::gate::GateMember>,
        pub created_at: u64,
        pub updated_at: u64,
        pub metadata: &'a Option<serde_json::Value>,
    }

    let key: surrealdb::sql::Thing = ("gates", gate.id.0.as_str()).into();
    let content = GateRecordContent {
        base: &gate.base,
        owner: &gate.owner,
        members: &gate.members,
        created_at: gate.created_at,
        updated_at: gate.updated_at,
        metadata: &gate.metadata,
    };

    // We don't need the typed record back; deserialize into generic JSON to avoid schema coupling.
    let _: Option<serde_json::Value> = db
        .update(key)
        .content(&content)
        .await
        .into_diagnostic()
        .wrap_err("upsert gate record")?;
    Ok(())
}

pub async fn get_gate(db: &Db, id: &GateId) -> miette::Result<Option<GateRecord>> {
    let key: surrealdb::sql::Thing = ("gates", id.0.as_str()).into();
    let res: Option<GateRow> = db
        .select(key)
        .await
        .into_diagnostic()
        .wrap_err("get gate record")?;
    Ok(res.map(row_to_gate_record))
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
    let rows: Vec<GateRow> = db
        .select("gates")
        .await
        .into_diagnostic()
        .wrap_err("list gate records")?;
    Ok(rows.into_iter().map(row_to_gate_record).collect())
}

pub async fn put_component(db: &Db, rec: &ComponentRecord) -> miette::Result<()> {
    // Avoid including the `id` field in the content payload to Surreal, as we are
    // already addressing the record by key. Mirror gate handling to prevent Surreal
    // from erroring on explicit id in content.
    #[derive(Serialize)]
    struct ComponentRecordContent<'a> {
        pub base: &'a component::Component,
        pub files: &'a crate::component::ComponentFiles,
        pub created_at: u64,
        pub updated_at: u64,
        pub metadata: &'a Option<serde_json::Value>,
    }

    let key: surrealdb::sql::Thing = ("components", rec.id.0.as_str()).into();
    let content = ComponentRecordContent {
        base: &rec.base,
        files: &rec.files,
        created_at: rec.created_at,
        updated_at: rec.updated_at,
        metadata: &rec.metadata,
    };

    let _: Option<serde_json::Value> = db
        .update(key)
        .content(&content)
        .await
        .into_diagnostic()
        .wrap_err("upsert component record")?;
    Ok(())
}

#[derive(Deserialize)]
struct ComponentRow {
    id: surrealdb::sql::Thing,
    base: component::Component,
    files: crate::component::ComponentFiles,
    created_at: u64,
    updated_at: u64,
    metadata: Option<serde_json::Value>,
}

fn thing_to_component_id(t: &surrealdb::sql::Thing) -> crate::types::ComponentId {
    use surrealdb::sql::Id;
    let s = match &t.id {
        Id::String(s) => s.clone(),
        other => other.to_string(),
    };
    crate::types::ComponentId(s)
}

fn row_to_component_record(r: ComponentRow) -> ComponentRecord {
    ComponentRecord {
        id: thing_to_component_id(&r.id),
        base: r.base,
        files: r.files,
        created_at: r.created_at,
        updated_at: r.updated_at,
        metadata: r.metadata,
    }
}

pub async fn get_component(db: &Db, id: &ComponentId) -> miette::Result<Option<ComponentRecord>> {
    let key: surrealdb::sql::Thing = ("components", id.0.as_str()).into();
    let res: Option<ComponentRow> = db
        .select(key)
        .await
        .into_diagnostic()
        .wrap_err("get component record")?;
    Ok(res.map(row_to_component_record))
}

pub async fn delete_component(db: &Db, id: &ComponentId) -> miette::Result<()> {
    let key: surrealdb::sql::Thing = ("components", id.0.as_str()).into();
    let _: Option<serde_json::Value> = db
        .delete(key)
        .await
        .into_diagnostic()
        .wrap_err("delete component record")?;
    Ok(())
}

pub async fn list_components(db: &Db) -> miette::Result<Vec<ComponentRecord>> {
    let rows: Vec<ComponentRow> = db
        .select("components")
        .await
        .into_diagnostic()
        .wrap_err("list component records")?;
    Ok(rows.into_iter().map(row_to_component_record).collect())
}
