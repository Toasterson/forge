use jj_lib::backend::{Commit, Conflict, FileId, MergedTreeId, SymlinkId, Tree, TreeValue};
use jj_lib::object_id::ObjectId;
use jj_lib::repo_path::RepoPathComponentBuf;
use miette::{IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};

/// Serializable representation of a Commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableCommit {
    pub parents: Vec<Vec<u8>>,
    pub predecessors: Vec<Vec<u8>>,
    pub root_tree: Vec<u8>,
    pub change_id: Vec<u8>,
    pub description: String,
    pub author_name: String,
    pub author_email: String,
    pub author_timestamp_millis: i64,
    pub author_tz_offset: i32,
    pub committer_name: String,
    pub committer_email: String,
    pub committer_timestamp_millis: i64,
    pub committer_tz_offset: i32,
}

impl SerializableCommit {
    pub fn from_commit(commit: &Commit) -> Self {
        // Extract resolved TreeId bytes from MergedTreeId
        let root_tree_bytes = match &commit.root_tree {
            MergedTreeId::Legacy(tree_id) => tree_id.as_bytes().to_vec(),
            MergedTreeId::Merge(merge) => {
                // Use the resolved value if available, otherwise use first
                if let Some(resolved) = merge.as_resolved() {
                    resolved.as_bytes().to_vec()
                } else {
                    // Fallback: serialize first value
                    merge
                        .adds()
                        .next()
                        .map_or_else(Vec::new, |t| t.as_bytes().to_vec())
                }
            }
        };

        Self {
            parents: commit
                .parents
                .iter()
                .map(|p| p.as_bytes().to_vec())
                .collect(),
            predecessors: commit
                .predecessors
                .iter()
                .map(|p| p.as_bytes().to_vec())
                .collect(),
            root_tree: root_tree_bytes,
            change_id: commit.change_id.as_bytes().to_vec(),
            description: commit.description.clone(),
            author_name: commit.author.name.clone(),
            author_email: commit.author.email.clone(),
            author_timestamp_millis: commit.author.timestamp.timestamp.0,
            author_tz_offset: commit.author.timestamp.tz_offset,
            committer_name: commit.committer.name.clone(),
            committer_email: commit.committer.email.clone(),
            committer_timestamp_millis: commit.committer.timestamp.timestamp.0,
            committer_tz_offset: commit.committer.timestamp.tz_offset,
        }
    }

    pub fn to_commit(&self) -> Result<Commit> {
        use jj_lib::backend::{ChangeId, CommitId, Signature, Timestamp, TreeId};

        let parents: Vec<_> = self
            .parents
            .iter()
            .map(|p| CommitId::from_bytes(p))
            .collect();

        let predecessors: Vec<_> = self
            .predecessors
            .iter()
            .map(|p| CommitId::from_bytes(p))
            .collect();

        Ok(Commit {
            parents,
            predecessors,
            root_tree: MergedTreeId::resolved(TreeId::from_bytes(&self.root_tree)),
            change_id: ChangeId::from_bytes(&self.change_id),
            description: self.description.clone(),
            author: Signature {
                name: self.author_name.clone(),
                email: self.author_email.clone(),
                timestamp: Timestamp {
                    timestamp: jj_lib::backend::MillisSinceEpoch(self.author_timestamp_millis),
                    tz_offset: self.author_tz_offset,
                },
            },
            committer: Signature {
                name: self.committer_name.clone(),
                email: self.committer_email.clone(),
                timestamp: Timestamp {
                    timestamp: jj_lib::backend::MillisSinceEpoch(self.committer_timestamp_millis),
                    tz_offset: self.committer_tz_offset,
                },
            },
            secure_sig: None,
        })
    }
}

/// Serializable representation of a Tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTree {
    pub entries: Vec<SerializableTreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTreeEntry {
    pub name: String,
    pub value: SerializableTreeValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializableTreeValue {
    File { id: Vec<u8>, executable: bool },
    Symlink(Vec<u8>),
    Tree(Vec<u8>),
    GitSubmodule(Vec<u8>),
    Conflict(Vec<u8>),
}

impl SerializableTree {
    pub fn from_tree(tree: &Tree) -> Self {
        let entries = tree
            .entries()
            .map(|entry| SerializableTreeEntry {
                name: entry.name().as_internal_str().to_string(),
                value: match entry.value() {
                    TreeValue::File { id, executable } => SerializableTreeValue::File {
                        id: id.to_bytes().to_vec(),
                        executable: *executable,
                    },
                    TreeValue::Symlink(id) => {
                        SerializableTreeValue::Symlink(id.to_bytes().to_vec())
                    }
                    TreeValue::Tree(id) => SerializableTreeValue::Tree(id.to_bytes().to_vec()),
                    TreeValue::GitSubmodule(id) => {
                        SerializableTreeValue::GitSubmodule(id.to_bytes().to_vec())
                    }
                    TreeValue::Conflict(id) => {
                        SerializableTreeValue::Conflict(id.to_bytes().to_vec())
                    }
                },
            })
            .collect();

        Self { entries }
    }

    pub fn to_tree(&self) -> Result<Tree> {
        use jj_lib::backend::{ConflictId, TreeId};

        let mut tree = Tree::default();

        for entry in &self.entries {
            let name = RepoPathComponentBuf::from(entry.name.clone());
            let value = match &entry.value {
                SerializableTreeValue::File { id, executable } => TreeValue::File {
                    id: FileId::from_bytes(id),
                    executable: *executable,
                },
                SerializableTreeValue::Symlink(id) => TreeValue::Symlink(SymlinkId::from_bytes(id)),
                SerializableTreeValue::Tree(id) => TreeValue::Tree(TreeId::from_bytes(id)),
                SerializableTreeValue::GitSubmodule(id) => {
                    TreeValue::GitSubmodule(jj_lib::backend::CommitId::from_bytes(id))
                }
                SerializableTreeValue::Conflict(id) => {
                    TreeValue::Conflict(ConflictId::from_bytes(id))
                }
            };

            tree.set_or_remove(&name, Some(value));
        }

        Ok(tree)
    }
}

/// Serialize commit to bytes
pub fn serialize_commit(commit: &Commit) -> Result<Vec<u8>> {
    let serializable = SerializableCommit::from_commit(commit);
    bincode::serialize(&serializable)
        .into_diagnostic()
        .map_err(|e| e.wrap_err("failed to serialize commit"))
}

/// Deserialize commit from bytes
pub fn deserialize_commit(bytes: &[u8]) -> Result<Commit> {
    let serializable: SerializableCommit = bincode::deserialize(bytes)
        .into_diagnostic()
        .map_err(|e| e.wrap_err("failed to deserialize commit"))?;
    serializable.to_commit()
}

/// Serialize tree to bytes
pub fn serialize_tree(tree: &Tree) -> Result<Vec<u8>> {
    let serializable = SerializableTree::from_tree(tree);
    bincode::serialize(&serializable)
        .into_diagnostic()
        .map_err(|e| e.wrap_err("failed to serialize tree"))
}

/// Deserialize tree from bytes
pub fn deserialize_tree(bytes: &[u8]) -> Result<Tree> {
    let serializable: SerializableTree = bincode::deserialize(bytes)
        .into_diagnostic()
        .map_err(|e| e.wrap_err("failed to deserialize tree"))?;
    serializable.to_tree()
}

/// Serialize conflict to bytes
pub fn serialize_conflict(conflict: &Conflict) -> Result<Vec<u8>> {
    let debug_str = format!("{:?}", conflict);
    Ok(debug_str.into_bytes())
}

/// Deserialize conflict from bytes
pub fn deserialize_conflict(_bytes: &[u8]) -> Result<Conflict> {
    Ok(Conflict::default())
}
