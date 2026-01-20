use jj_lib::object_id::ObjectId;
use serde::{Deserialize, Serialize};
use std::fmt;

// ========== IDs ==========

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GateId(pub String);

impl fmt::Display for GateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentId(pub String);

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActorId(pub String);

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReplicaId(pub String);

impl fmt::Display for ReplicaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ========== VCS Types ==========

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChangeId(pub jj_lib::backend::ChangeId);

impl ChangeId {
    pub fn hex(&self) -> String {
        self.0.hex()
    }

    pub fn inner(&self) -> &jj_lib::backend::ChangeId {
        &self.0
    }
}

impl From<jj_lib::backend::ChangeId> for ChangeId {
    fn from(id: jj_lib::backend::ChangeId) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationId(pub jj_lib::op_store::OperationId);

impl OperationId {
    pub fn hex(&self) -> String {
        hex::encode(self.0.as_bytes())
    }

    pub fn inner(&self) -> &jj_lib::op_store::OperationId {
        &self.0
    }
}

impl From<jj_lib::op_store::OperationId> for OperationId {
    fn from(id: jj_lib::op_store::OperationId) -> Self {
        Self(id)
    }
}

// ========== Content Addressing ==========

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(bytes);
        Self(hash.into())
    }

    pub fn hex(&self) -> String {
        hex::encode(&self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("invalid hash length"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

// ========== Actor Types ==========

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Service,
}

impl fmt::Display for ActorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActorKind::User => write!(f, "user"),
            ActorKind::Service => write!(f, "service"),
        }
    }
}

impl std::str::FromStr for ActorKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" | "User" => Ok(ActorKind::User),
            "service" | "Service" => Ok(ActorKind::Service),
            _ => Err(format!("invalid actor kind: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActorRef {
    pub id: ActorId,
    pub kind: ActorKind,
}

// ========== Repository References ==========

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RepoId {
    Gate(GateId),
    Component(ComponentId),
}

impl fmt::Display for RepoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoId::Gate(id) => write!(f, "gate:{}", id),
            RepoId::Component(id) => write!(f, "component:{}", id),
        }
    }
}

impl Serialize for RepoId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RepoId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if let Some(rest) = s.strip_prefix("gate:") {
            Ok(RepoId::Gate(GateId(rest.to_string())))
        } else if let Some(rest) = s.strip_prefix("component:") {
            Ok(RepoId::Component(ComponentId(rest.to_string())))
        } else {
            Err(serde::de::Error::custom(format!(
                "invalid repo id format: {}",
                s
            )))
        }
    }
}

// ========== Revision References ==========

#[derive(Debug, Clone)]
pub enum Revision {
    Operation(OperationId),
    Change(ChangeId),
    Ref(String), // "@", "main", etc.
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Revision::Operation(id) => write!(f, "op:{}", id.hex()),
            Revision::Change(id) => write!(f, "change:{}", id.hex()),
            Revision::Ref(r) => write!(f, "ref:{}", r),
        }
    }
}
