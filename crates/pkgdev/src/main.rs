use clap::Parser;
use pkgdev::args::run;
use pkgdev::args::Args;

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();
    run(args).await?;
    Ok(())
}
