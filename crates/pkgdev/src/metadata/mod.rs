use miette::IntoDiagnostic;

use crate::args::ComponentArgs;

mod repology;

pub(crate) fn print_component(args: ComponentArgs) -> miette::Result<()> {
    let component = component::Component::open_local(&args.component)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&component).into_diagnostic()?
    );
    Ok(())
}
