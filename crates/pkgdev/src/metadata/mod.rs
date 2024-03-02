use clap::ValueEnum;
use miette::IntoDiagnostic;

use crate::args::ComponentArgs;

mod repology;

#[derive(Debug, ValueEnum, Clone, Default)]
pub(crate) enum MetadataFormat {
    #[default]
    Forge,
    Repology,
}

pub(crate) fn print_component(args: ComponentArgs, format: MetadataFormat) -> miette::Result<()> {
    let component = component::Component::open_local(&args.component)?;
    match format {
        MetadataFormat::Forge => {
            println!(
                "{}",
                serde_json::to_string_pretty(&component).into_diagnostic()?
            );
        }
        MetadataFormat::Repology => {
            let r = repology::build_metadata(&component)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&r).into_diagnostic()?
            )
        }
    }
    Ok(())
}
