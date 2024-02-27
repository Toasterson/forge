use clap::Parser;

use crate::args::run;

mod args;
mod create;
mod metadata;
mod modify;
mod sources;

fn main() -> miette::Result<()> {
    let args = args::Args::parse();
    run(args)?;
    Ok(())
}
