use clap::Parser;

use crate::args::run;

mod args;
mod metadata;

fn main() -> miette::Result<()> {
    let args = args::Args::parse();
    run(args)?;
    Ok(())
}
