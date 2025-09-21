use miette::Diagnostic;
use thiserror::Error;

use component::{Component, SourceNode};
use repology::MetadataBuilder;

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug, Diagnostic)]
pub enum RepologyError {
    #[error("component has no summary")]
    MissingSummary,
    #[error("no project name in component")]
    MissingProjectName,
    #[error("no project url set in the component")]
    MissingProjectUrl,
    #[error("no license set in the component")]
    MissingLicense,
    #[error("no version set in the component")]
    MissingVersion,
    #[error("no category set in the component")]
    MissingCategory,
}

pub(crate) fn build_metadata(c: &Component) -> miette::Result<repology::Metadata> {
    let recipe = &c.recipe;
    let m = MetadataBuilder::default()
        .summary(
            recipe
                .summary
                .clone()
                .ok_or(RepologyError::MissingSummary)?,
        )
        .fmri(recipe.name.clone())
        .project_name(
            recipe
                .project_name
                .clone()
                .ok_or(RepologyError::MissingProjectName)?,
        )
        .add_homepage(
            recipe
                .project_url
                .clone()
                .ok_or(RepologyError::MissingProjectUrl)?,
        )
        .add_license(
            recipe
                .license
                .clone()
                .ok_or(RepologyError::MissingLicense)?,
        )
        .version(
            recipe
                .version
                .clone()
                .ok_or(RepologyError::MissingVersion)?,
        )
        .source_links(
            recipe
                .sources
                .iter()
                .flat_map(|s| {
                    s.sources
                        .iter()
                        .filter_map(|so| match so {
                            SourceNode::Archive(a) => Some(a.src.clone()),
                            SourceNode::Git(g) => Some(g.repository.clone()),
                            SourceNode::File(_) => None,
                            SourceNode::Directory(_) => None,
                            SourceNode::Patch(_) => None,
                            SourceNode::Overlay(_) => None,
                        })
                        .collect::<Vec<String>>()
                })
                .collect::<Vec<String>>(),
        )
        .add_category(
            recipe
                .classification
                .clone()
                .ok_or(RepologyError::MissingCategory)?,
        )
        .build()?;
    Ok(m)
}
