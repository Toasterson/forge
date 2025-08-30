use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::{
    fs::{read_to_string, File},
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum GateError {
    #[error(transparent)]
    #[diagnostic(code(gate::io))]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    #[diagnostic(code(bundle::kdl_error))]
    Kdl(#[from] kdl::KdlError),
    #[error(transparent)]
    #[diagnostic(code(bundle::url_parse_error))]
    UrlParseError(#[from] url::ParseError),
    #[error("the path {0} cannot be opened as gate file plese provide the correct path to a gate.kdl file")]
    NoFileNameError(String),
    #[error("only one packages with name {0} should be present. There are {1}")]
    TooManyPackagesWithTheSameName(String, usize),
    #[error("no package with name {0}")]
    NoSuchPackage(String),
    #[error("distribution type {0} is not known use one of 'tarball', 'ips'")]
    UnknownDistributionType(String),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Knuffel(#[from] knuffel::Error),
}

type GateResult<T> = Result<T, GateError>;

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize)]
pub struct Gate {
    path: PathBuf,
    #[knuffel(child, unwrap(argument))]
    pub id: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub name: String,
    #[knuffel(child, unwrap(argument))]
    pub version: String,
    #[knuffel(child, unwrap(argument))]
    pub branch: String,
    #[knuffel(child)]
    pub distribution: Option<Distribution>,
    #[knuffel(children(name = "transform"))]
    pub default_transforms: Vec<Transform>,
    #[knuffel(child, unwrap(argument))]
    pub publisher: String,
    #[knuffel(children(name = "metadata-transform"))]
    pub metadata_transforms: Vec<MetadataTransform>,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            id: None,
            path: PathBuf::new(),
            name: String::new(),
            version: String::from("0.5.11"),
            branch: String::from("2023.0.0"),
            distribution: None,
            default_transforms: vec![],
            publisher: String::from("userland"),
            metadata_transforms: vec![],
        }
    }
}

impl Gate {
    /// Create an empty gate
    pub fn empty<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            ..Default::default()
        }
    }

    /// Read file at path and return the Gate object
    ///
    /// # Errors
    ///
    /// Fails if reading the file fails or parsing the Gate content
    pub fn load<P: AsRef<Path>>(path: P) -> GateResult<Self> {
        let path = if path.as_ref().is_absolute() {
            path.as_ref().to_path_buf()
        } else {
            path.as_ref().canonicalize()?
        };

        let gate_document_contents = read_to_string(&path)?;
        let name = path
            .file_name()
            .ok_or(GateError::NoFileNameError(
                path.to_string_lossy().to_string(),
            ))?
            .to_string_lossy()
            .to_string();

        let mut gate = knuffel::parse::<Self>(&name, &gate_document_contents)?;
        gate.path = path;
        Ok(gate)
    }

    #[must_use]
    pub fn to_document(&self) -> kdl::KdlDocument {
        let mut node = self.to_node();
        node.ensure_children().clone()
    }

    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("gate");
        let doc = node.ensure_children();

        if let Some(id) = &self.id {
            let mut id_node = kdl::KdlNode::new("id");
            id_node.insert(0, id.as_str());
            doc.nodes_mut().push(id_node);
        }

        let mut name_node = kdl::KdlNode::new("name");
        name_node.insert(0, self.name.as_str());
        doc.nodes_mut().push(name_node);

        let mut version_node = kdl::KdlNode::new("version");
        version_node.insert(0, self.version.as_str());
        doc.nodes_mut().push(version_node);

        let mut branch_node = kdl::KdlNode::new("branch");
        branch_node.insert(0, self.branch.as_str());
        doc.nodes_mut().push(branch_node);

        let mut publisher_node = kdl::KdlNode::new("publisher");
        publisher_node.insert(0, self.publisher.as_str());
        doc.nodes_mut().push(publisher_node);

        if let Some(distribution) = &self.distribution {
            let distribution_node = distribution.to_node();
            doc.nodes_mut().push(distribution_node);
        }

        for tr in &self.default_transforms {
            let tr_node = tr.to_node();
            doc.nodes_mut().push(tr_node);
        }

        for mt in &self.metadata_transforms {
            let meta_node = mt.to_node();
            doc.nodes_mut().push(meta_node);
        }

        node
    }

    /// Save the current Gate back to the Filesystem
    ///
    /// # Errors
    ///
    /// Can error while serializing or saving to disk
    pub fn save(&self) -> GateResult<()> {
        let doc = self.to_document();
        let mut f = File::create(&self.path)?;
        f.write_all(doc.to_string().as_bytes())?;
        Ok(())
    }

    #[must_use]
    pub fn get_gate_path(&self) -> PathBuf {
        self.path
            .parent()
            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
    }
}

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize)]
pub struct MetadataTransform {
    #[knuffel(property)]
    pub matcher: String,
    #[knuffel(property, default = String::new())]
    pub replacement: String,
    #[knuffel(property, default = false)]
    pub drop: bool,
}

impl MetadataTransform {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("metadata-transform");
        let matcher_prop = kdl::KdlEntry::new_prop("matcher", self.matcher.as_str());
        let replacement_prop = kdl::KdlEntry::new_prop("replacement", self.replacement.as_str());
        node.push(matcher_prop);
        node.push(replacement_prop);
        if self.drop {
            let drop_prop = kdl::KdlEntry::new_prop("drop", self.drop);
            node.push(drop_prop);
        }
        node
    }
}

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize)]
pub struct Transform {
    // Legacy textual actions (pkgmogrify lines). Kept for backward compatibility.
    #[knuffel(arguments)]
    actions: Vec<String>,
    // Optional include file path (legacy include semantics)
    #[knuffel(property)]
    include: Option<String>,
    // New: KDL-based, typed transform rules encoded as child nodes
    #[knuffel(children(name = "rule"))]
    pub rules: Vec<TransformRuleAst>,
}

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize)]
pub struct TransformRuleAst {
    #[knuffel(children(name = "select"))]
    pub selectors: Vec<TransformSelectorAst>,
    #[knuffel(children(name = "op"))]
    pub ops: Vec<TransformOpAst>,
}

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize)]
pub struct TransformSelectorAst {
    // The IPS action type to match (e.g., file, link, hardlink, dir). Optional to allow attr-only matches.
    #[knuffel(property, str)]
    pub action: Option<String>,
    // Attribute name to match on (e.g., path, mode, owner)
    #[knuffel(property, str)]
    pub attr: Option<String>,
    // Pattern or exact string to match
    #[knuffel(property(name = "pattern"), str)]
    pub pat: Option<String>,
}

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize)]
pub struct TransformOpAst {
    // Operation name, e.g., set, delete, drop, default
    #[knuffel(argument)]
    pub name: String,
    // Optional key for set/delete/default operations
    #[knuffel(property, default = None, str)]
    pub key: Option<String>,
    // Optional value for set/default operations
    #[knuffel(property, default = None, str)]
    pub value: Option<String>,
}

impl Transform {
    #[must_use]
    pub fn to_transform_line(&self) -> String {
        // Preserve legacy behavior for callers that still expect textual mogrify lines
        let mut lines = self.actions.clone();
        if let Some(include_prop) = &self.include {
            lines.push(format!("<include {include_prop}>"));
        }
        lines.join("\n")
    }

    /// Access typed KDL rules if provided
    pub fn ast_rules(&self) -> &[TransformRuleAst] {
        &self.rules
    }

    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("transform");
        for (idx, action) in self.actions.iter().enumerate() {
            node.insert(idx, action.as_str());
        }
        if let Some(include_prop) = &self.include {
            node.insert("include", include_prop.as_str());
        }
        // Serialize typed rules back into KDL children
        let doc = node.ensure_children();
        for rule in &self.rules {
            let mut rule_node = kdl::KdlNode::new("rule");
            let rule_doc = rule_node.ensure_children();
            for sel in &rule.selectors {
                let mut sel_node = kdl::KdlNode::new("select");
                if let Some(action) = &sel.action {
                    sel_node.insert("action", action.as_str());
                }
                if let Some(attr) = &sel.attr {
                    sel_node.insert("attr", attr.as_str());
                }
                if let Some(pat) = &sel.pat {
                    sel_node.insert("pattern", pat.as_str());
                }
                rule_doc.nodes_mut().push(sel_node);
            }
            for op in &rule.ops {
                let mut op_node = kdl::KdlNode::new("op");
                op_node.insert(0, op.name.as_str());
                if let Some(key) = &op.key {
                    op_node.insert("key", key.as_str());
                }
                if let Some(value) = &op.value {
                    op_node.insert("value", value.as_str());
                }
                rule_doc.nodes_mut().push(op_node);
            }
            doc.nodes_mut().push(rule_node);
        }
        node
    }
}

#[derive(Debug, Default, knuffel::Decode, Clone, Serialize, Deserialize)]
pub struct Distribution {
    #[knuffel(property(name = "type"), default, str)]
    pub distribution_type: DistributionType,
}

impl Distribution {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("distribution");
        let doc = node.ensure_children();
        let mut type_node = kdl::KdlNode::new("type");
        type_node.insert(0, self.distribution_type.to_string().as_str());
        doc.nodes_mut().push(type_node);
        node
    }
}

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize)]
pub enum DistributionType {
    Tarbball,
    IPS,
}

impl Default for DistributionType {
    fn default() -> Self {
        Self::IPS
    }
}

impl FromStr for DistributionType {
    type Err = GateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tarball" | "tar" => Ok(Self::Tarbball),
            "ips" | "IPS" => Ok(Self::IPS),
            x => Err(GateError::UnknownDistributionType(x.to_string())),
        }
    }
}

impl Display for DistributionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tarbball => write!(f, "tarball"),
            Self::IPS => write!(f, "ips"),
        }
    }
}
