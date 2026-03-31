use clap::Parser;
use pkgdev::args::run;
use pkgdev::args::Args;

#[tokio::main]
async fn main() -> miette::Result<()> {
    // Initialize tracing subscriber with env filter support.
    // Configure via RUST_LOG, e.g.: RUST_LOG=pkgdev=info
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let args = Args::parse();
    run(args).await?;
    Ok(())
}
