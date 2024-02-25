use std::path::PathBuf;

use clap::Parser;
use miette::IntoDiagnostic;

mod repology;

#[derive(Debug, Parser)]
pub(crate) struct Args {
    #[clap(short, long, default_value = ".")]
    component: PathBuf,
}

pub(crate) fn print_component(args: Args) -> miette::Result<()> {
    let component = component::Component::open_local(&args.component)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&component).into_diagnostic()?
    );
    Ok(())
}
