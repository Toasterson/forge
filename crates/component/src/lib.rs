use derive_builder::Builder;
use diff::Diff;
use kdl::KdlValue;
use miette::{Diagnostic, IntoDiagnostic, WrapErr};
use schemars::schema::RootSchema;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::{
    fs::{read_to_string, File},
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;
use utoipa::ToSchema;

#[derive(Error, Debug, Diagnostic)]
pub enum ComponentError {
    #[error(transparent)]
    #[diagnostic(code(component::io_error))]
    IOError(#[from] std::io::Error),

    #[error("no parent directory of package.kdl exists")]
    NoPackageDocumentParentDir,

    #[error(transparent)]
    #[diagnostic(transparent)]
    Kdl(#[from] kdl::KdlError),

    #[error(transparent)]
    #[diagnostic(code(bundle::url_parse_error))]
    UrlParseError(#[from] url::ParseError),

    #[error("unknown build type {0}")]
    UnknownBuildType(String),

    #[error("build types {0} and {1} are not mergeable")]
    NonMergeableBuildSections(String, String),

    #[error(transparent)]
    UninitializedFieldError(#[from] derive_builder::UninitializedFieldError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Knuffel(#[from] knuffel::Error),
}

type ComponentResult<T> = Result<T, ComponentError>;

pub fn get_schema() -> RootSchema {
    schema_for!(Component)
}

#[derive(Debug, Clone, Serialize, Deserialize, Diff, PartialEq, JsonSchema)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct Component {
    path: PathBuf,
    pub recipe: Recipe,
    pub package_meta: Option<PackageMeta>,
}

impl Component {
    pub fn new<P: AsRef<Path>>(name: String, p: Option<P>) -> ComponentResult<Self> {
        let path = if let Some(p) = p {
            p.as_ref().to_path_buf()
        } else {
            PathBuf::from(".")
        };

        Ok(Self {
            path,
            recipe: RecipeBuilder::default().name(name).build()?,
            package_meta: None,
        })
    }

    pub fn open_local<P: AsRef<Path>>(path: P) -> ComponentResult<Self> {
        let path = path.as_ref().canonicalize()?;

        let (package_document_string, name, dir) = if path.is_file() {
            (
                read_to_string(path.clone())?,
                path.parent()
                    .ok_or(ComponentError::NoPackageDocumentParentDir)?
                    .to_string_lossy()
                    .to_string(),
                path.parent()
                    .ok_or(ComponentError::NoPackageDocumentParentDir)?,
            )
        } else {
            (
                read_to_string(&path.join("package.kdl"))?,
                path.to_string_lossy().to_string(),
                path.as_path(),
            )
        };

        let package_meta_path = dir.join("pkg5");
        let package_meta = if package_meta_path.exists() {
            let file = File::open(&package_meta_path)?;
            serde_json::from_reader(file).ok()
        } else {
            None
        };

        if path.is_file() {
            let package_document = knuffel::parse::<Recipe>(&name, &package_document_string)?;
            Ok(Self {
                path: dir.to_path_buf(),
                recipe: package_document,
                package_meta,
            })
        } else {
            let package_document = knuffel::parse::<Recipe>(&name, &package_document_string)?;
            Ok(Self {
                path,
                recipe: package_document,
                package_meta,
            })
        }
    }

    fn open_document(&mut self) -> miette::Result<()> {
        let data_string = read_to_string(&self.path.join("package.kdl"))
            .into_diagnostic()
            .wrap_err("could not open package document")?;
        self.recipe = knuffel::parse::<Recipe>("package.kdl", &data_string)?;
        Ok(())
    }

    pub fn save_document(&self) -> ComponentResult<()> {
        let doc_str = self.recipe.to_document().to_string();
        let mut f = File::create(&self.path.join("package.kdl"))?;
        f.write_all(doc_str.as_bytes())?;
        Ok(())
    }

    pub fn add_source(&mut self, node: SourceNode) -> miette::Result<()> {
        if let Some(src_section) = self.recipe.sources.first_mut() {
            src_section.sources.push(node);
        } else {
            let src_section = SourceSection {
                sources: vec![node],
            };
            self.recipe.sources.push(src_section);
        };
        self.save_document()?;
        self.open_document()?;
        Ok(())
    }

    pub fn get_path(&self) -> &Path {
        &self.path
    }

    pub fn get_name(&self) -> String {
        self.recipe.name.clone()
    }

    pub fn get_mogrify_manifest(&self) -> Option<PathBuf> {
        let file_path = self.path.join("manifest.mog");
        if file_path.exists() {
            Some(file_path)
        } else {
            None
        }
    }
}

#[derive(
    Debug,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    Builder,
    Diff,
    PartialEq,
    JsonSchema,
    ToSchema,
)]
#[builder(setter(into, strip_option), build_fn(error = "self::ComponentError"))]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct PackageMeta {
    name: String,
    fmris: Vec<String>,
    dependencies: Vec<String>,
}

impl Default for PackageMeta {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            fmris: vec![],
            dependencies: vec![],
        }
    }
}

#[derive(
    Debug,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    Builder,
    Diff,
    PartialEq,
    JsonSchema,
    ToSchema,
)]
#[builder(setter(into, strip_option), build_fn(error = "self::ComponentError"))]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct ComponentMetadataItem {
    #[knuffel(node_name)]
    pub name: String,
    #[knuffel(argument)]
    pub value: String,
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, Diff, PartialEq, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct ComponentMetadata(#[knuffel(children)] pub Vec<ComponentMetadataItem>);

#[derive(
    Debug,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    Builder,
    Diff,
    PartialEq,
    ToSchema,
    JsonSchema,
)]
#[builder(setter(into, strip_option), build_fn(error = "self::ComponentError"))]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct Recipe {
    #[knuffel(child, unwrap(argument))]
    pub name: String,

    #[knuffel(child)]
    #[builder(default)]
    pub metadata: Option<ComponentMetadata>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub project_name: Option<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub classification: Option<String>,

    #[knuffel(children(name = "maintainer"), unwrap(argument))]
    #[builder(default)]
    pub maintainers: Vec<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub summary: Option<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub license_file: Option<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub license: Option<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub prefix: Option<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub version: Option<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub revision: Option<String>,

    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub project_url: Option<String>,

    #[knuffel(child)]
    #[builder(default)]
    pub seperate_build_dir: bool,

    #[knuffel(children(name = "source"))]
    #[builder(default)]
    pub sources: Vec<SourceSection>,

    #[knuffel(children(name = "dependency"))]
    #[builder(default)]
    pub dependencies: Vec<Dependency>,

    #[knuffel(children(name = "build"))]
    #[builder(default)]
    pub build_sections: Vec<BuildSection>,

    #[knuffel(children(name = "package"))]
    #[builder(default)]
    pub package_sections: Vec<PackageSection>,
}

impl Display for Recipe {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}@{}-{}",
            self.name,
            self.version.clone().unwrap_or("0.1.0".to_string()),
            self.revision.clone().unwrap_or("0".to_string())
        )
    }
}

impl Recipe {
    pub fn to_document(&self) -> kdl::KdlDocument {
        let pkg_node = self.to_node();
        pkg_node
            .children()
            .unwrap_or(&kdl::KdlDocument::new())
            .clone()
    }

    pub fn insert_metadata(&mut self, key: &str, value: &str) {
        if self.metadata.is_none() {
            self.metadata = Some(ComponentMetadata(vec![]));
        }
        if let Some(metadata) = &mut self.metadata {
            metadata.0.push(ComponentMetadataItem {
                name: key.to_string(),
                value: value.to_string(),
            });
        }
    }

    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("package");
        let doc = node.ensure_children();
        let mut name_node = kdl::KdlNode::new("name");
        name_node.insert(0, self.name.as_str());
        doc.nodes_mut().push(name_node);

        if let Some(project_name) = &self.project_name {
            let mut project_name_node = kdl::KdlNode::new("project-name");
            project_name_node.insert(0, project_name.as_str());
            doc.nodes_mut().push(project_name_node);
        }

        if let Some(metadata) = &self.metadata {
            let mut metadata_node = kdl::KdlNode::new("metadata");
            for item in &metadata.0 {
                let mut item_node = kdl::KdlNode::new(item.name.clone());
                item_node.insert(0, item.value.clone());
                metadata_node.ensure_children().nodes_mut().push(item_node);
            }
            doc.nodes_mut().push(metadata_node);
        }

        if let Some(classification) = &self.classification {
            let mut classification_node = kdl::KdlNode::new("classification");
            classification_node.insert(0, classification.as_str());
            doc.nodes_mut().push(classification_node);
        }

        if let Some(summary) = &self.summary {
            let mut summary_node = kdl::KdlNode::new("summary");
            summary_node.insert(0, summary.as_str());
            doc.nodes_mut().push(summary_node);
        }

        if let Some(license_file) = &self.license_file {
            let mut license_file_node = kdl::KdlNode::new("license-file");
            license_file_node.insert(0, license_file.as_str());
            doc.nodes_mut().push(license_file_node);
        }

        if let Some(license) = &self.license {
            let mut license_node = kdl::KdlNode::new("license");
            license_node.insert(0, license.as_str());
            doc.nodes_mut().push(license_node);
        }

        if let Some(prefix) = &self.prefix {
            let mut prefix_node = kdl::KdlNode::new("prefix");
            prefix_node.insert(0, prefix.as_str());
            doc.nodes_mut().push(prefix_node);
        }

        if let Some(version) = &self.version {
            let mut version_node = kdl::KdlNode::new("version");
            version_node.insert(0, version.as_str());
            doc.nodes_mut().push(version_node);
        }

        if let Some(revision) = &self.revision {
            let mut revision_node = kdl::KdlNode::new("revision");
            revision_node.insert(0, revision.as_str());
            doc.nodes_mut().push(revision_node);
        }

        if let Some(project_url) = &self.project_url {
            let mut project_url_node = kdl::KdlNode::new("project-url");
            project_url_node.insert(0, project_url.as_str());
            doc.nodes_mut().push(project_url_node);
        }

        for maintainer in self.maintainers.iter() {
            let mut maintainer_node = kdl::KdlNode::new("maintainer");
            maintainer_node.insert(0, maintainer.as_str());
            doc.nodes_mut().push(maintainer_node);
        }

        for src in &self.sources {
            let source_node = src.to_node();
            doc.nodes_mut().push(source_node);
        }

        for build in &self.build_sections {
            let build_node = build.to_node();
            doc.nodes_mut().push(build_node);
        }

        for dependency in &self.dependencies {
            let dep_node = dependency.to_node();
            doc.nodes_mut().push(dep_node);
        }

        for package in &self.package_sections {
            let package_node = package.to_node();
            doc.nodes_mut().push(package_node);
        }

        node
    }

    pub fn merge_into_mut(&mut self, other: &Recipe) -> ComponentResult<()> {
        self.name = other.name.clone();

        if let Some(classification) = &other.classification {
            self.classification = Some(classification.clone());
        }

        if let Some(summary) = &other.summary {
            self.summary = Some(summary.clone());
        }

        if let Some(license_file) = &other.license_file {
            self.license_file = Some(license_file.clone());
        }

        if let Some(license) = &other.license {
            self.license = Some(license.clone());
        }

        if let Some(prefix) = &other.prefix {
            self.prefix = Some(prefix.clone());
        }

        if let Some(version) = &other.version {
            self.version = Some(version.clone());
        }

        if let Some(revision) = &other.revision {
            self.revision = Some(revision.clone());
        }

        if let Some(project_url) = &other.project_url {
            self.project_url = Some(project_url.clone());
        }

        for maintainer in &other.maintainers {
            self.maintainers.push(maintainer.clone());
        }

        for bld in &other.build_sections {
            self.build_sections.push(bld.clone());
        }

        for src in &other.sources {
            self.sources.push(src.clone());
        }

        for dep in &other.dependencies {
            self.dependencies.push(dep.clone());
        }

        Ok(())
    }
}

#[derive(
    Debug,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    JsonSchema,
    Builder,
    ToSchema,
)]
#[builder(setter(into, strip_option), build_fn(error = "self::ComponentError"))]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct Dependency {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(property, default = false)]
    pub dev: bool,
    #[knuffel(property)]
    #[builder(default)]
    pub kind: DependencyKind,
}

impl Dependency {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("dependency");
        node.insert(0, self.name.as_str());

        if self.dev {
            node.insert("dev", true);
        }

        node.insert("kind", &self.kind);

        node
    }
}

#[derive(
    Debug,
    knuffel::DecodeScalar,
    Default,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    JsonSchema,
    ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub enum DependencyKind {
    #[default]
    Require,
    Incorporate,
    Optional,
}

impl From<&DependencyKind> for KdlValue {
    fn from(value: &DependencyKind) -> Self {
        match value {
            DependencyKind::Require => "require".into(),
            DependencyKind::Incorporate => "incorporate".into(),
            DependencyKind::Optional => "optional".into(),
        }
    }
}

impl From<&str> for DependencyKind {
    fn from(value: &str) -> Self {
        match value {
            "require" => Self::Require,
            "incorporate" => Self::Incorporate,
            "optional" => Self::Optional,
            _ => Self::Require,
        }
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct SourceSection {
    #[knuffel(children)]
    pub sources: Vec<SourceNode>,
}

impl SourceSection {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut source_node = kdl::KdlNode::new("source");

        for src in &self.sources {
            let src_node = match src {
                SourceNode::Archive(s) => s.to_node(),
                SourceNode::Git(s) => s.to_node(),
                SourceNode::File(s) => s.to_node(),
                SourceNode::Patch(s) => s.to_node(),
                SourceNode::Overlay(s) => s.to_node(),
                SourceNode::Directory(s) => s.to_node(),
            };
            let doc = source_node.ensure_children();
            doc.nodes_mut().push(src_node);
        }

        source_node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub enum SourceNode {
    Archive(ArchiveSource),
    Git(GitSource),
    File(FileSource),
    Directory(DirectorySource),
    Patch(PatchSource),
    Overlay(OverlaySource),
}

#[derive(
    Debug,
    Default,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    JsonSchema,
    Builder,
    ToSchema,
)]
#[builder(setter(into, strip_option), build_fn(error = "self::ComponentError"))]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct ArchiveSource {
    #[knuffel(argument)]
    pub src: String,

    #[knuffel(property)]
    #[builder(default)]
    pub sha512: Option<String>,

    #[knuffel(property)]
    #[builder(default)]
    pub sha256: Option<String>,

    #[knuffel(property)]
    #[builder(default)]
    pub signature_url_extension: Option<String>,

    #[knuffel(property)]
    #[builder(default)]
    pub signature_url: Option<String>,
}

impl ArchiveSource {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("archive");
        node.insert(0, self.src.as_str());
        if let Some(sha512) = &self.sha512 {
            node.insert("sha512", sha512.as_str());
        }
        if let Some(sha256) = &self.sha256 {
            node.insert("sha256", sha256.as_str());
        }
        if let Some(signature_ext) = &self.signature_url_extension {
            node.insert("singature-url-extension", signature_ext.as_str());
        }
        if let Some(sig_url) = &self.signature_url {
            node.insert("signature-url", sig_url.as_str());
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct GitSource {
    #[knuffel(argument)]
    pub repository: String,
    #[knuffel(property)]
    pub branch: Option<String>,
    #[knuffel(property)]
    pub tag: Option<String>,
    #[knuffel(property)]
    pub archive: Option<bool>,
    #[knuffel(property)]
    pub must_stay_as_repo: Option<bool>,

    // Directory where to unpack sources into the first git source can ignore this on the second it is required
    #[knuffel(property)]
    pub directory: Option<String>,
}

impl GitSource {
    pub fn get_repo_prefix(&self) -> String {
        let repo_prefix_part = self
            .repository
            .rsplit_once('/')
            .unwrap_or(("", &self.repository))
            .1;
        let repo_prefix = if let Some(split_sucess) = repo_prefix_part.split_once('.') {
            split_sucess.0.to_string()
        } else {
            repo_prefix_part.to_string()
        };

        if let Some(tag) = &self.tag {
            format!("{}-{}", repo_prefix, tag)
        } else if let Some(branch) = &self.branch {
            format!("{}-{}", repo_prefix, branch)
        } else {
            format!("{}", repo_prefix)
        }
    }

    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("git");
        node.insert(0, self.repository.as_str());
        if let Some(branch) = &self.branch {
            node.insert("branch", branch.as_str());
        }
        if let Some(tag) = &self.tag {
            node.insert("tag", tag.as_str());
        }
        if let Some(archive) = self.archive.clone() {
            node.insert("archive", archive);
        }
        if let Some(must_stay_as_repo) = self.must_stay_as_repo.clone() {
            node.insert("must-stay-as-repo", must_stay_as_repo);
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct FileSource {
    #[schema(value_type = String)]
    #[knuffel(argument)]
    bundle_path: String,
    #[knuffel(argument)]
    target_path: Option<String>,
}

impl FileSource {
    pub fn new(bundle_path: String, target_path: Option<String>) -> ComponentResult<Self> {
        Ok(Self {
            bundle_path,
            target_path,
        })
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    pub fn get_target_path(&self) -> PathBuf {
        if let Some(p) = &self.target_path {
            PathBuf::from(p)
        } else {
            PathBuf::from(&self.bundle_path)
        }
    }

    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("file");
        node.insert(0, self.bundle_path.as_str());
        if let Some(target_path) = &self.target_path {
            node.insert(1, target_path.as_str());
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct DirectorySource {
    #[schema(value_type = String)]
    #[knuffel(argument)]
    bundle_path: String,
    #[knuffel(argument)]
    target_path: Option<String>,
}

impl DirectorySource {
    pub fn new(bundle_path: String, target_path: Option<String>) -> ComponentResult<Self> {
        Ok(Self {
            bundle_path,
            target_path,
        })
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    pub fn get_name(&self) -> String {
        self.bundle_path.clone()
    }

    pub fn get_target_path(&self) -> PathBuf {
        if let Some(p) = &self.target_path {
            PathBuf::from(p)
        } else {
            PathBuf::from(&self.bundle_path)
        }
    }

    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("directory");
        node.insert(0, self.bundle_path.as_str());
        if let Some(target_path) = &self.target_path {
            node.insert(1, target_path.as_str());
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct PatchSource {
    #[schema(value_type = String)]
    #[knuffel(argument)]
    bundle_path: String,
    #[knuffel(property)]
    pub drop_directories: Option<i64>,
}

impl Display for PatchSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.bundle_path.as_str())
    }
}

impl PatchSource {
    pub fn new(bundle_path: String, drop_directories: Option<i64>) -> ComponentResult<Self> {
        Ok(Self {
            bundle_path,
            drop_directories,
        })
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("patch");
        node.insert(0, self.bundle_path.as_str());
        if let Some(dirs) = self.drop_directories.clone() {
            node.insert("drop-directories", dirs);
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct OverlaySource {
    #[knuffel(argument)]
    bundle_path: String,
}

impl OverlaySource {
    pub fn new(bundle_path: String) -> ComponentResult<Self> {
        Ok(Self { bundle_path })
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("overlay");
        node.insert(0, self.bundle_path.as_str());
        node
    }
}

#[derive(
    Debug,
    Default,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    JsonSchema,
    Builder,
    ToSchema,
)]
#[builder(setter(into, strip_option), build_fn(error = "self::ComponentError"))]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct BuildSection {
    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub source: Option<String>,
    #[knuffel(child)]
    #[builder(default)]
    pub configure: Option<ConfigureBuildSection>,
    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub cmake: Option<String>,
    #[knuffel(child, unwrap(argument))]
    #[builder(default)]
    pub meson: Option<String>,
    #[knuffel(child)]
    #[builder(default)]
    pub script: Option<ScriptBuildSection>,
}

impl BuildSection {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("build");
        if let Some(source) = &self.source {
            node.insert(0, source.as_str());
        }
        let doc = node.ensure_children();
        if let Some(configure) = &self.configure {
            doc.nodes_mut().push(configure.to_node());
        } else if let Some(script) = &self.script {
            doc.nodes_mut().push(script.to_node());
        } else {
            doc.nodes_mut().push(kdl::KdlNode::new("no-build"));
        }
        node
    }
}

#[derive(
    Debug,
    Default,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    ToSchema,
    JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct ConfigureBuildSection {
    #[knuffel(children(name = "option"))]
    pub options: Vec<BuildOptionNode>,
    #[knuffel(children(name = "flag"))]
    pub flags: Vec<BuildFlagNode>,
    #[knuffel(child, unwrap(argument))]
    pub compiler: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub linker: Option<String>,
    #[knuffel(child, default = false)]
    pub disable_destdir_configure_option: bool,
    #[knuffel(child, default = false)]
    pub enable_large_files: bool,
}

impl ConfigureBuildSection {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("configure");
        let doc = node.ensure_children();
        for option in &self.options {
            doc.nodes_mut().push(option.to_node());
        }

        for flag in &self.flags {
            doc.nodes_mut().push(flag.to_node());
        }

        if let Some(compiler) = &self.compiler {
            let mut n = kdl::KdlNode::new("compiler");
            n.insert(0, compiler.clone());
            doc.nodes_mut().push(n);
        }

        if let Some(linker) = &self.linker {
            let mut n = kdl::KdlNode::new("linker");
            n.insert(0, linker.clone());
            doc.nodes_mut().push(n);
        }

        if self.disable_destdir_configure_option {
            let n = kdl::KdlNode::new("disable-destdir-option");
            doc.nodes_mut().push(n);
        }

        if self.enable_large_files {
            let n = kdl::KdlNode::new("enable-large-files");
            doc.nodes_mut().push(n);
        }

        node
    }
}

#[derive(
    Debug,
    Default,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    ToSchema,
    JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct ScriptBuildSection {
    #[knuffel(children(name = "script"))]
    pub scripts: Vec<ScriptNode>,
    #[knuffel(children(name = "install"))]
    pub install_directives: Vec<InstallDirectiveNode>,
}

impl ScriptBuildSection {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("script");
        let doc = node.ensure_children();
        for script in &self.scripts {
            doc.nodes_mut().push(script.to_node());
        }

        for install in &self.install_directives {
            doc.nodes_mut().push(install.to_node());
        }

        node
    }
}

#[derive(
    Debug,
    Default,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    ToSchema,
    JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct InstallDirectiveNode {
    #[knuffel(property)]
    pub src: String,
    #[knuffel(property)]
    pub target: String,
    #[knuffel(property)]
    pub name: String,
    #[knuffel(property)]
    pub pattern: Option<String>,
    #[knuffel(property(name = "match"))]
    pub fmatch: Option<String>,
}

impl InstallDirectiveNode {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("package-directory");
        node.insert("src", self.src.as_str());
        node.insert("target", self.target.as_str());
        node.insert("name", self.name.as_str());
        node
    }
}

#[derive(
    Debug,
    Default,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Diff,
    ToSchema,
    JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct ScriptNode {
    #[knuffel(argument)]
    pub name: String,
    #[schema(value_type = String)]
    #[knuffel(property)]
    pub prototype_dir: Option<String>,
}

impl ScriptNode {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("script");
        node.insert(0, self.name.as_str());
        if let Some(prototype_dir) = &self.prototype_dir {
            node.insert("prototype-dir", prototype_dir.as_str());
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct BuildFlagNode {
    #[knuffel(argument)]
    pub flag: String,
    #[knuffel(property(name = "name"))]
    pub flag_name: Option<String>,
}

impl BuildFlagNode {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("flag");
        node.insert(0, self.flag.as_str());
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct BuildOptionNode {
    #[knuffel(argument)]
    pub option: String,
}

impl BuildOptionNode {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("option");
        node.insert(0, self.option.as_str());
        node
    }
}

#[derive(Debug, knuffel::Decode, Clone, Serialize, Deserialize, ToSchema, Diff)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct FileNode {
    #[knuffel(child, unwrap(argument))]
    pub include: String,
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, PartialEq, Deserialize, ToSchema, Diff, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct PackageSection {
    #[knuffel(argument)]
    pub name: Option<String>,

    #[knuffel(children(name = "file"))]
    pub files: Vec<TransformNode>,

    #[knuffel(children(name = "link"))]
    pub links: Vec<TransformNode>,

    #[knuffel(children(name = "hardlinks"))]
    pub hardlinks: Vec<TransformNode>,
}

impl PackageSection {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("package");
        if let Some(name) = &self.name {
            node.insert(0, name.as_str());
        }

        let doc = node.ensure_children();

        for file in &self.files {
            doc.nodes_mut().push(file.to_node());
        }

        for link in &self.links {
            doc.nodes_mut().push(link.to_node());
        }

        for hardlink in &self.hardlinks {
            doc.nodes_mut().push(hardlink.to_node());
        }

        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, PartialEq, Deserialize, ToSchema, Diff, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct TransformNode {
    #[knuffel(node_name)]
    pub action: String,
    #[knuffel(properties)]
    pub selectors: HashMap<String, String>,
}

impl TransformNode {
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new(self.action.as_str());

        for selector in &self.selectors {
            node.insert(selector.0.as_str(), selector.1.as_str());
        }

        node
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use miette::IntoDiagnostic;

    use crate::*;

    /// Find all the bundle files at the given path. This will search the path
    /// recursively for any file named `package.kdl`.
    pub fn find_bundle_files(path: &Path) -> ComponentResult<Vec<PathBuf>> {
        let mut result = Vec::new();
        find_bundle_files_rec(path, &mut result)?;
        Ok(result)
    }

    /// Search the file system recursively for all build files.
    fn find_bundle_files_rec(path: &Path, result: &mut Vec<PathBuf>) -> ComponentResult<()> {
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            let ft = e.file_type()?;
            if ft.is_symlink() {
                continue;
            } else if ft.is_dir() {
                find_bundle_files_rec(&e.path(), result)?;
            } else if e.file_name() == "package.kdl" {
                result.push(e.path());
            }
        }

        Ok(())
    }

    #[test]
    fn test_read_all_samples() -> miette::Result<()> {
        let paths = find_bundle_files(Path::new("../packages")).into_diagnostic()?;
        let bundles = paths
            .into_iter()
            .map(|path| Component::open_local(&path))
            .collect::<ComponentResult<Vec<Component>>>()?;
        for bundle in bundles {
            assert_ne!(bundle.recipe.name, String::from(""))
        }

        Ok(())
    }

    #[test]
    fn parse_openssl() -> miette::Result<()> {
        let bundle_path = Path::new("../packages/openssl");
        let _b = Component::open_local(bundle_path)?;

        Ok(())
    }

    #[test]
    fn parse_binutils_gdb() -> miette::Result<()> {
        let bundle_path = Path::new("../packages/binutils-gdb");
        let _b = Component::open_local(bundle_path)?;

        Ok(())
    }
}
