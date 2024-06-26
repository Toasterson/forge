use clap::Parser;
use forged::*;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
                "forge=trace,receiver=trace,github=trace,tower_http=trace,axum::rejection=trace"
                    .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let cfg = load_config(&args)?;
    match &args.command {
        Commands::Start => {
            listen(cfg).await?;
        }
        Commands::GenDomain {
            name, gh_client_id, ..
        } => {
            gen_domain(cfg, name.clone(), gh_client_id.clone()).await?;
        }
        Commands::SetDomain {
            name, gh_client_id, ..
        } => {
            set_domain(cfg, name.clone(), gh_client_id.clone()).await?;
        }
    }

    Ok(())
}
