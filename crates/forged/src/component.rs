use crate::types::ComponentId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
// Alias the external `component` crate to avoid name collision with this module
use ::component as base_component;

/// Represents a file associated with a component (e.g., patch, license, install script).
/// Paths are relative to the component base path or repository root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredFile {
    /// Display name or file name
    pub name: String,
    /// Relative path to the file in storage or VCS
    pub rel_path: String,
}

/// Logical grouping of associated files for a component.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentFiles {
    /// Patches applied to the component sources
    pub patches: Vec<StoredFile>,
    /// Licenses relevant to the component
    pub licenses: Vec<StoredFile>,
    /// Install/packaging scripts
    pub scripts: Vec<StoredFile>,
}

/// Server-side Component record that composes the base model from the `component` crate
/// and enriches it with server-side management data and associated files used for packaging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRecord {
    /// Stable identifier for this component within the server.
    pub id: ComponentId,
    /// The base component model (from component crate).
    pub base: base_component::Component,
    /// Associated files used for packaging such as patches, licenses, scripts.
    #[serde(default)]
    pub files: ComponentFiles,
    /// Creation timestamp (seconds since epoch).
    pub created_at: u64,
    /// Update timestamp (seconds since epoch).
    pub updated_at: u64,
    /// Arbitrary server-side metadata.
    pub metadata: Option<serde_json::Value>,
}

impl ComponentRecord {
    pub fn new(id: ComponentId, base: base_component::Component) -> Self {
        let now = now_sec();
        Self {
            id,
            base,
            files: ComponentFiles::default(),
            created_at: now,
            updated_at: now,
            metadata: None,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_sec();
    }
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

    #[test]
    fn serialize_roundtrip() {
        let base =
            base_component::Component::new("zlib".to_string(), None::<&std::path::Path>).unwrap();
        let mut rec = ComponentRecord::new(ComponentId("c-zlib".into()), base);
        rec.files.patches.push(StoredFile {
            name: "01-fix.patch".into(),
            rel_path: "patches/01-fix.patch".into(),
        });
        rec.files.licenses.push(StoredFile {
            name: "LICENSE".into(),
            rel_path: "LICENSE".into(),
        });
        rec.files.scripts.push(StoredFile {
            name: "install.sh".into(),
            rel_path: "scripts/install.sh".into(),
        });
        let s = serde_json::to_string_pretty(&rec).unwrap();
        let de: ComponentRecord = serde_json::from_str(&s).unwrap();
        assert_eq!(de.id.0, "c-zlib");
        assert_eq!(de.files.patches.len(), 1);
        assert_eq!(de.files.licenses.len(), 1);
        assert_eq!(de.files.scripts.len(), 1);
    }
}
