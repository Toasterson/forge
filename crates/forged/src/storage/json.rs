use miette::{Context, IntoDiagnostic};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A very simple JSON file-based key-value store.
/// Each key is stored as a file `<root>/<key>.json`.
/// Keys should be filesystem-safe (we do minimal validation).
#[derive(Debug, Clone)]
pub struct JsonStore {
    root: PathBuf,
}

impl JsonStore {
    /// Create or open a JSON store under the given root directory.
    pub fn new<P: AsRef<Path>>(root: P) -> miette::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .into_diagnostic()
            .wrap_err("create json store root directory")?;
        Ok(Self { root })
    }

    fn key_path(&self, key: &str) -> miette::Result<PathBuf> {
        // very basic validation to avoid path traversal
        if key.contains("..") || key.contains('/') || key.contains('\\') {
            miette::bail!("invalid key: contains path separators or traversal");
        }
        Ok(self.root.join(format!("{}.json", key)))
    }

    /// Store a serializable value at the given key.
    pub fn put<T: Serialize>(&self, key: &str, value: &T) -> miette::Result<()> {
        let path = self.key_path(key)?;
        let tmp_path = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(value)
            .into_diagnostic()
            .wrap_err("serialize value to json")?;
        {
            let mut f = fs::File::create(&tmp_path)
                .into_diagnostic()
                .wrap_err_with(|| format!("create temp file for {}", key))?;
            f.write_all(&data)
                .into_diagnostic()
                .wrap_err("write json data")?;
            f.sync_all().into_diagnostic().wrap_err("sync temp file")?;
        }
        fs::rename(&tmp_path, &path)
            .into_diagnostic()
            .wrap_err("atomic rename temp->final")?;
        Ok(())
    }

    /// Retrieve and deserialize the value at key. Returns Ok(None) if not found.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> miette::Result<Option<T>> {
        let path = self.key_path(key)?;
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path)
            .into_diagnostic()
            .wrap_err_with(|| format!("read {}", path.display()))?;
        let value = serde_json::from_slice(&data)
            .into_diagnostic()
            .wrap_err("deserialize json")?;
        Ok(Some(value))
    }

    /// Delete a key if it exists.
    pub fn delete(&self, key: &str) -> miette::Result<()> {
        let path = self.key_path(key)?;
        if path.exists() {
            fs::remove_file(&path)
                .into_diagnostic()
                .wrap_err("remove json file")?;
        }
        Ok(())
    }
}

use crate::auth::{Session, SessionStore};

/// SessionStore implementation backed by JsonStore
#[derive(Debug, Clone)]
pub struct JsonSessionStore {
    inner: JsonStore,
}

impl JsonSessionStore {
    pub fn new<P: AsRef<Path>>(root: P) -> miette::Result<Self> {
        Ok(Self {
            inner: JsonStore::new(root)?,
        })
    }

    fn key_for(id: &str) -> String {
        format!("session-{}", id)
    }
}

impl SessionStore for JsonSessionStore {
    fn put(&self, session: &Session) -> anyhow::Result<()> {
        self.inner
            .put(&Self::key_for(&session.id), session)
            .map_err(|e| anyhow::anyhow!(e))
    }

    fn get(&self, id: &str) -> anyhow::Result<Option<Session>> {
        self.inner
            .get(&Self::key_for(id))
            .map_err(|e| anyhow::anyhow!(e))
    }

    fn revoke(&self, id: &str) -> anyhow::Result<()> {
        self.inner
            .delete(&Self::key_for(id))
            .map_err(|e| anyhow::anyhow!(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "forged-jsonstore-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn json_store_roundtrip() {
        let root = test_dir("kv");
        let store = JsonStore::new(&root).unwrap();
        store.put("foo", &serde_json::json!({"a": 1})).unwrap();
        let v: serde_json::Value = store.get("foo").unwrap().unwrap();
        assert_eq!(v["a"], 1);
        store.delete("foo").unwrap();
        assert!(store.get::<serde_json::Value>("foo").unwrap().is_none());
    }

    #[test]
    fn session_store_roundtrip() {
        let root = test_dir("session");
        let sess_store = JsonSessionStore::new(&root).unwrap();
        let sess = Session::new("user-123".to_string(), Duration::from_secs(60));
        sess_store.put(&sess).unwrap();
        let got = sess_store.get(&sess.id).unwrap().unwrap();
        assert_eq!(got.subject, sess.subject);
        sess_store.revoke(&sess.id).unwrap();
        assert!(sess_store.get(&sess.id).unwrap().is_none());
    }
}

// ---- Gate and Component repositories backed by JsonStore ----
use crate::component::ComponentRecord;
use crate::gate::GateRecord;
use crate::storage::{ComponentStore, GateStore};
use crate::types::{ComponentId, GateId};

#[derive(Debug, Clone)]
pub struct JsonGateStore {
    inner: JsonStore,
}

impl JsonGateStore {
    pub fn new<P: AsRef<Path>>(root: P) -> miette::Result<Self> {
        Ok(Self {
            inner: JsonStore::new(root)?,
        })
    }
    fn key_for(id: &GateId) -> String {
        format!("gate-{}", id.0)
    }
}

impl GateStore for JsonGateStore {
    fn put_gate(&self, gate: &GateRecord) -> miette::Result<()> {
        self.inner.put(&Self::key_for(&gate.id), gate)
    }
    fn get_gate(&self, id: &GateId) -> miette::Result<Option<GateRecord>> {
        self.inner.get(&Self::key_for(id))
    }
    fn delete_gate(&self, id: &GateId) -> miette::Result<()> {
        self.inner.delete(&Self::key_for(id))
    }
}

#[derive(Debug, Clone)]
pub struct JsonComponentStore {
    inner: JsonStore,
}

impl JsonComponentStore {
    pub fn new<P: AsRef<Path>>(root: P) -> miette::Result<Self> {
        Ok(Self {
            inner: JsonStore::new(root)?,
        })
    }
    fn key_for(id: &ComponentId) -> String {
        format!("component-{}", id.0)
    }
}

impl ComponentStore for JsonComponentStore {
    fn put_component(&self, id: &ComponentId, component: &ComponentRecord) -> miette::Result<()> {
        self.inner.put(&Self::key_for(id), component)
    }
    fn get_component(&self, id: &ComponentId) -> miette::Result<Option<ComponentRecord>> {
        self.inner.get(&Self::key_for(id))
    }
    fn delete_component(&self, id: &ComponentId) -> miette::Result<()> {
        self.inner.delete(&Self::key_for(id))
    }
}

#[cfg(test)]
mod repo_tests {
    use super::*;
    use crate::types::{ActorId, ActorKind, ActorRef};

    fn test_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "forged-jsonstore-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn gate_repo_roundtrip() {
        let root = test_dir("gate-repo");
        let store = JsonGateStore::new(&root).unwrap();
        let base = gate::Gate::default();
        let rec = GateRecord::new(
            GateId("g1".into()),
            base,
            ActorRef {
                id: ActorId("owner".into()),
                kind: ActorKind::User,
            },
        );
        store.put_gate(&rec).unwrap();
        let got = store.get_gate(&GateId("g1".into())).unwrap().unwrap();
        assert_eq!(got.id.0, "g1");
        store.delete_gate(&GateId("g1".into())).unwrap();
        assert!(store.get_gate(&GateId("g1".into())).unwrap().is_none());
    }

    #[test]
    fn component_repo_roundtrip() {
        let root = test_dir("component-repo");
        let store = JsonComponentStore::new(&root).unwrap();
        let base =
            ::component::Component::new("hello".to_string(), None::<&std::path::Path>).unwrap();
        let id = ComponentId("c1".into());
        let rec = ComponentRecord::new(id.clone(), base);
        store.put_component(&id, &rec).unwrap();
        let got = store.get_component(&id).unwrap().unwrap();
        assert_eq!(got.base.get_name(), rec.base.get_name());
        store.delete_component(&id).unwrap();
        assert!(store.get_component(&id).unwrap().is_none());
    }
}
