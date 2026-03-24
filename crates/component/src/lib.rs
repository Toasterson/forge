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

#[must_use]
pub fn get_schema() -> RootSchema {
    schema_for!(Component)
}

#[derive(Debug, Clone, Serialize, Deserialize, Diff, PartialEq, Eq, JsonSchema)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct Component {
    path: PathBuf,
    pub recipe: Recipe,
    pub package_meta: Option<PackageMeta>,
}

impl Component {
    /// Build a new Component with given name
    ///
    /// # Errors
    ///
    /// Returns an error if the Recipe builder fails
    pub fn new<P: AsRef<Path>>(name: String, p: Option<P>) -> ComponentResult<Self> {
        let path = p.map_or_else(|| PathBuf::from("."), |p| p.as_ref().to_path_buf());

        Ok(Self {
            path,
            recipe: RecipeBuilder::default().name(name).build()?,
            package_meta: None,
        })
    }

    /// Open a Local Component
    ///
    /// # Errors
    ///
    /// Can fail to read from disk or deserialize the package.kdl file or the pkg5 json file
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
                read_to_string(path.join("package.kdl"))?,
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

        let package_document = knuffel::parse::<Recipe>(&name, &package_document_string)?;
        if path.is_file() {
            Ok(Self {
                path: dir.to_path_buf(),
                recipe: package_document,
                package_meta,
            })
        } else {
            Ok(Self {
                path,
                recipe: package_document,
                package_meta,
            })
        }
    }

    fn open_document(&mut self) -> miette::Result<()> {
        let data_string = read_to_string(self.path.join("package.kdl"))
            .into_diagnostic()
            .wrap_err("could not open package document")?;
        self.recipe = knuffel::parse::<Recipe>("package.kdl", &data_string)?;
        Ok(())
    }

    /// Save the Component to disk
    ///
    /// # Errors
    ///
    /// Can fail to serialize or save to disk
    pub fn save_document(&self) -> ComponentResult<()> {
        let doc_str = self.recipe.to_document().to_string();
        let mut f = File::create(self.path.join("package.kdl"))?;
        f.write_all(doc_str.as_bytes())?;
        Ok(())
    }

    /// Add a source node to the Component and save the file
    ///
    /// # Errors
    ///
    /// Can fail to serialize and write to disk
    pub fn add_source(&mut self, node: SourceNode) -> miette::Result<()> {
        if let Some(src_section) = self.recipe.sources.first_mut() {
            src_section.sources.push(node);
        } else {
            let src_section = SourceSection {
                sources: vec![node],
            };
            self.recipe.sources.push(src_section);
        }
        self.save_document()?;
        self.open_document()?;
        Ok(())
    }

    #[must_use]
    pub fn get_path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn get_name(&self) -> String {
        self.recipe.name.clone()
    }

    #[must_use]
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
    Eq,
    JsonSchema,
    ToSchema,
    Default,
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

#[derive(
    Debug,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    Builder,
    Diff,
    PartialEq,
    Eq,
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
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, Diff, PartialEq, Eq, ToSchema, JsonSchema,
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
    Eq,
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

    #[knuffel(children(name = "group"))]
    #[builder(default)]
    pub groups: Vec<GroupAction>,

    #[knuffel(children(name = "user"))]
    #[builder(default)]
    pub users: Vec<UserAction>,

    #[knuffel(children(name = "driver"))]
    #[builder(default)]
    pub drivers: Vec<DriverAction>,

    #[knuffel(children(name = "files"))]
    #[builder(default)]
    pub file_sections: Vec<FilesSection>,
}

impl Display for Recipe {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}@{}-{}",
            self.name,
            self.version.clone().unwrap_or_else(|| "0.1.0".to_string()),
            self.revision.clone().unwrap_or_else(|| "0".to_string())
        )
    }
}

impl Recipe {
    #[must_use]
    pub fn to_document(&self) -> kdl::KdlDocument {
        let mut pkg_node = self.to_node();
        pkg_node.ensure_children().clone()
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

    #[must_use]
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

        for maintainer in &self.maintainers {
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

        for group in &self.groups {
            doc.nodes_mut().push(group.to_node());
        }

        for user in &self.users {
            doc.nodes_mut().push(user.to_node());
        }

        for driver in &self.drivers {
            doc.nodes_mut().push(driver.to_node());
        }

        for files in &self.file_sections {
            doc.nodes_mut().push(files.to_node());
        }

        node
    }

    pub fn merge_into_mut(&mut self, other: &Self) {
        self.name.clone_from(&other.name);

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

        for group in &other.groups {
            self.groups.push(group.clone());
        }

        for user in &other.users {
            self.users.push(user.clone());
        }

        for driver in &other.drivers {
            self.drivers.push(driver.clone());
        }

        for files in &other.file_sections {
            self.file_sections.push(files.clone());
        }
    }
}

#[derive(
    Debug,
    knuffel::Decode,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
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
    #[must_use]
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
    Eq,
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
    Group,
}

impl From<&DependencyKind> for KdlValue {
    fn from(value: &DependencyKind) -> Self {
        match value {
            DependencyKind::Require => "require".into(),
            DependencyKind::Incorporate => "incorporate".into(),
            DependencyKind::Optional => "optional".into(),
            DependencyKind::Group => "group".into(),
        }
    }
}

#[allow(clippy::match_same_arms)]
impl From<&str> for DependencyKind {
    fn from(value: &str) -> Self {
        match value {
            "require" => Self::Require,
            "incorporate" => Self::Incorporate,
            "optional" => Self::Optional,
            "group" => Self::Group,
            _ => Self::Require,
        }
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct SourceSection {
    #[knuffel(children)]
    pub sources: Vec<SourceNode>,
}

impl SourceSection {
    #[must_use]
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
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
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
    Eq,
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
    #[must_use]
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
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
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
    #[must_use]
    pub fn get_repo_prefix(&self) -> String {
        let repo_prefix_part = self
            .repository
            .rsplit_once('/')
            .unwrap_or(("", &self.repository))
            .1;
        let repo_prefix = repo_prefix_part.split_once('.').map_or_else(
            || repo_prefix_part.to_string(),
            |split_sucess| split_sucess.0.to_string(),
        );

        self.tag.as_ref().map_or_else(
            || {
                self.branch.as_ref().map_or_else(
                    || repo_prefix.to_string(),
                    |branch| format!("{repo_prefix}-{branch}"),
                )
            },
            |tag| format!("{repo_prefix}-{tag}"),
        )
    }

    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("git");
        node.insert(0, self.repository.as_str());
        if let Some(branch) = &self.branch {
            node.insert("branch", branch.as_str());
        }
        if let Some(tag) = &self.tag {
            node.insert("tag", tag.as_str());
        }
        if let Some(archive) = self.archive {
            node.insert("archive", archive);
        }
        if let Some(must_stay_as_repo) = self.must_stay_as_repo {
            node.insert("must-stay-as-repo", must_stay_as_repo);
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
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
    #[must_use]
    pub const fn new(bundle_path: String, target_path: Option<String>) -> Self {
        Self {
            bundle_path,
            target_path,
        }
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    #[must_use]
    pub fn get_target_path(&self) -> PathBuf {
        self.target_path
            .as_ref()
            .map_or_else(|| PathBuf::from(&self.bundle_path), PathBuf::from)
    }

    #[must_use]
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
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
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
    #[must_use]
    pub const fn new(bundle_path: String, target_path: Option<String>) -> Self {
        Self {
            bundle_path,
            target_path,
        }
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    #[must_use]
    pub fn get_name(&self) -> String {
        self.bundle_path.clone()
    }

    #[must_use]
    pub fn get_target_path(&self) -> PathBuf {
        self.target_path
            .as_ref()
            .map_or_else(|| PathBuf::from(&self.bundle_path), PathBuf::from)
    }

    #[must_use]
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
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
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
    #[must_use]
    pub const fn new(bundle_path: String, drop_directories: Option<i64>) -> Self {
        Self {
            bundle_path,
            drop_directories,
        }
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("patch");
        node.insert(0, self.bundle_path.as_str());
        if let Some(dirs) = self.drop_directories {
            node.insert("drop-directories", dirs);
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct OverlaySource {
    #[knuffel(argument)]
    bundle_path: String,
}

impl OverlaySource {
    #[must_use]
    pub const fn new(bundle_path: String) -> Self {
        Self { bundle_path }
    }

    pub fn get_bundle_path<P: AsRef<Path>>(&self, base_path: P) -> PathBuf {
        base_path.as_ref().join(&self.bundle_path)
    }

    #[must_use]
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
    Eq,
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
    #[knuffel(child)]
    #[builder(default)]
    pub cargo: Option<CargoBuildSection>,
}

impl BuildSection {
    #[must_use]
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
        } else if let Some(cargo) = &self.cargo {
            doc.nodes_mut().push(cargo.to_node());
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
    Eq,
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
    #[must_use]
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
    Eq,
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
    #[must_use]
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
    Eq,
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
    #[must_use]
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
    Eq,
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
    #[must_use]
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
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
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
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("flag");
        node.insert(0, self.flag.as_str());
        if let Some(name) = &self.flag_name {
            node.insert("name", name.as_str());
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, ToSchema, JsonSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct BuildOptionNode {
    #[knuffel(argument)]
    pub option: String,
}

impl BuildOptionNode {
    #[must_use]
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
    Debug, knuffel::Decode, Clone, Serialize, PartialEq, Eq, Deserialize, ToSchema, Diff, JsonSchema,
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

    #[knuffel(children(name = "hardlink"))]
    pub hardlinks: Vec<TransformNode>,

    #[knuffel(children(name = "dir"))]
    pub dirs: Vec<TransformNode>,
}

impl PackageSection {
    #[must_use]
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

        for dir in &self.dirs {
            doc.nodes_mut().push(dir.to_node());
        }

        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, PartialEq, Eq, Deserialize, ToSchema, Diff, JsonSchema,
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
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new(self.action.as_str());

        for selector in &self.selectors {
            node.insert(selector.0.as_str(), selector.1.as_str());
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
    Eq,
    Diff,
    JsonSchema,
    ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct CargoBuildSection {
    #[knuffel(child, unwrap(arguments))]
    pub packages: Vec<String>,

    #[knuffel(child, unwrap(arguments))]
    pub features: Vec<String>,

    #[knuffel(child, unwrap(argument))]
    pub install_root: Option<String>,

    #[knuffel(child, default = true)]
    pub offline: bool,

    #[knuffel(child, default = true)]
    pub locked: bool,

    #[knuffel(child, unwrap(argument))]
    pub target: Option<String>,

    #[knuffel(children(name = "env"))]
    pub env_vars: Vec<EnvVarNode>,
}

impl CargoBuildSection {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("cargo");
        let doc = node.ensure_children();

        if !self.packages.is_empty() {
            let mut packages_node = kdl::KdlNode::new("packages");
            for (i, pkg) in self.packages.iter().enumerate() {
                packages_node.insert(i, pkg.as_str());
            }
            doc.nodes_mut().push(packages_node);
        }

        if !self.features.is_empty() {
            let mut features_node = kdl::KdlNode::new("features");
            for (i, feat) in self.features.iter().enumerate() {
                features_node.insert(i, feat.as_str());
            }
            doc.nodes_mut().push(features_node);
        }

        if let Some(install_root) = &self.install_root {
            let mut n = kdl::KdlNode::new("install-root");
            n.insert(0, install_root.as_str());
            doc.nodes_mut().push(n);
        }

        if self.offline {
            doc.nodes_mut().push(kdl::KdlNode::new("offline"));
        }

        if self.locked {
            doc.nodes_mut().push(kdl::KdlNode::new("locked"));
        }

        if let Some(target) = &self.target {
            let mut n = kdl::KdlNode::new("target");
            n.insert(0, target.as_str());
            doc.nodes_mut().push(n);
        }

        for env_var in &self.env_vars {
            doc.nodes_mut().push(env_var.to_node());
        }

        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, JsonSchema, ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct EnvVarNode {
    #[knuffel(properties)]
    pub vars: HashMap<String, String>,
}

impl EnvVarNode {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("env");
        for (key, value) in &self.vars {
            node.insert(key.as_str(), value.as_str());
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, JsonSchema, ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct GroupAction {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(property)]
    pub gid: i64,
}

impl GroupAction {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("group");
        node.insert(0, self.name.as_str());
        node.insert("gid", self.gid);
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, JsonSchema, ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct UserAction {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(property)]
    pub uid: i64,
    #[knuffel(property)]
    pub group: String,
    #[knuffel(property)]
    pub home: Option<String>,
    #[knuffel(property)]
    pub shell: Option<String>,
    #[knuffel(property)]
    pub description: Option<String>,
    #[knuffel(property, default = false)]
    pub ftpuser: bool,
}

impl UserAction {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("user");
        node.insert(0, self.name.as_str());
        node.insert("uid", self.uid);
        node.insert("group", self.group.as_str());
        if let Some(home) = &self.home {
            node.insert("home", home.as_str());
        }
        if let Some(shell) = &self.shell {
            node.insert("shell", shell.as_str());
        }
        if let Some(description) = &self.description {
            node.insert("description", description.as_str());
        }
        if self.ftpuser {
            node.insert("ftpuser", true);
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, JsonSchema, ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct DriverAction {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(child, unwrap(argument))]
    pub perms: Option<String>,
    #[knuffel(children(name = "devlink"), unwrap(argument))]
    pub devlinks: Vec<String>,
    #[knuffel(children(name = "alias"), unwrap(argument))]
    pub aliases: Vec<String>,
    #[knuffel(child, unwrap(argument))]
    pub class: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub policy: Option<String>,
}

impl DriverAction {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("driver");
        node.insert(0, self.name.as_str());
        let doc = node.ensure_children();

        if let Some(perms) = &self.perms {
            let mut n = kdl::KdlNode::new("perms");
            n.insert(0, perms.as_str());
            doc.nodes_mut().push(n);
        }

        for devlink in &self.devlinks {
            let mut n = kdl::KdlNode::new("devlink");
            n.insert(0, devlink.as_str());
            doc.nodes_mut().push(n);
        }

        for alias in &self.aliases {
            let mut n = kdl::KdlNode::new("alias");
            n.insert(0, alias.as_str());
            doc.nodes_mut().push(n);
        }

        if let Some(class) = &self.class {
            let mut n = kdl::KdlNode::new("class");
            n.insert(0, class.as_str());
            doc.nodes_mut().push(n);
        }

        if let Some(policy) = &self.policy {
            let mut n = kdl::KdlNode::new("policy");
            n.insert(0, policy.as_str());
            doc.nodes_mut().push(n);
        }

        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, JsonSchema, ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct FilesSection {
    #[knuffel(children(name = "install"))]
    pub installs: Vec<InstallFile>,
}

impl FilesSection {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("files");
        let doc = node.ensure_children();
        for install in &self.installs {
            doc.nodes_mut().push(install.to_node());
        }
        node
    }
}

#[derive(
    Debug, knuffel::Decode, Clone, Serialize, Deserialize, PartialEq, Eq, Diff, JsonSchema, ToSchema,
)]
#[diff(attr(
# [derive(Debug, Clone, Serialize, Deserialize)]
))]
pub struct InstallFile {
    #[knuffel(argument)]
    pub src: String,
    #[knuffel(argument)]
    pub dest: String,
    #[knuffel(property)]
    pub mode: Option<String>,
}

impl InstallFile {
    #[must_use]
    pub fn to_node(&self) -> kdl::KdlNode {
        let mut node = kdl::KdlNode::new("install");
        node.insert(0, self.src.as_str());
        node.insert(1, self.dest.as_str());
        if let Some(mode) = &self.mode {
            node.insert("mode", mode.as_str());
        }
        node
    }
}
