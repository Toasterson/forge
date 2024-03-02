use miette::Diagnostic;
use semver::Version;
use thiserror::Error;
use component::{Component, SourceNode};
use repology::MetadataBuilder;

#[derive(Error, Debug, Diagnostic)]
pub enum RepologyError {
    #[error("component has no summary")]
    NoSummary,
    #[error("no project name in component")]
    NoProjectName,
    #[error("no project url set in the component")]
    NoProjectUrl,
    #[error("no license set in the component")]
    NoLicense,
    #[error("no version set in the component")]
    NoVersion,
    #[error("no category set in the component")]
    NoCategory,
}

pub(crate) fn build_metadata(c: &Component) -> miette::Result<repology::Metadata> {
    let recipe = &c.recipe;
    let m = MetadataBuilder::default()
        .summary(recipe.summary.clone().ok_or(RepologyError::NoSummary)?)
        .fmri(recipe.name.clone())
        .project_name(recipe.project_name.clone().ok_or(RepologyError::NoProjectName)?)
        .add_homepage(recipe.project_url.clone().ok_or(RepologyError::NoProjectUrl)?)
        .add_license(recipe.license.clone().ok_or(RepologyError::NoLicense)?)
        .version(Version::parse(&recipe.version.clone().ok_or(RepologyError::NoVersion)?)?)
        .source_links(recipe.sources.iter().map(|s| {s.sources.iter().filter_map(|so|{match so {
            SourceNode::Archive(a) => Some(a.src.clone()),
            SourceNode::Git(g) => Some(g.repository.clone()),
            SourceNode::File(_) => None,
            SourceNode::Directory(_) => None,
            SourceNode::Patch(_) => None,
            SourceNode::Overlay(_) => None,
        }}).collect()}).flatten().collect())
        .add_category(recipe.classification.clone().ok_or(RepologyError::NoCategory)?)
        .build()?;
    Ok(m)
}