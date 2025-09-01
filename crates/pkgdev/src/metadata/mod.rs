use clap::ValueEnum;
use gate::Gate;
use miette::IntoDiagnostic;
use strum::Display;

use crate::args::ComponentArgs;
use crate::component::open_component_local;

mod repology;

#[derive(Debug, ValueEnum, Clone, Default, Display)]
pub enum MetadataFormat {
    #[default]
    Forge,
    Repology,
}

pub fn print_component(
    args: ComponentArgs,
    format: MetadataFormat,
    gate: &Option<Gate>,
) -> miette::Result<()> {
    let component = open_component_local(&args.component, gate)?;
    match format {
        MetadataFormat::Forge => {
            println!(
                "{}",
                serde_json::to_string_pretty(&component).into_diagnostic()?
            );
        }
        MetadataFormat::Repology => {
            let r = repology::build_metadata(&component)?;
            println!("{}", serde_json::to_string_pretty(&r).into_diagnostic()?)
        }
    }
    Ok(())
}
