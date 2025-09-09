use miette::{Context, IntoDiagnostic};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_tracing(service_name: &str) -> miette::Result<()> {
    // Base fmt + env filter
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    #[cfg(feature = "otel")]
    {
        use opentelemetry::KeyValue;
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
        use tracing_opentelemetry::layer;

        let mut registry = tracing_subscriber::registry().with(env_filter);

        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            let resource = Resource::new(vec![KeyValue::new(
                "service.name",
                service_name.to_string(),
            )]);
            let provider = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(
                    opentelemetry_otlp::new_exporter()
                        .tonic()
                        .with_endpoint(endpoint),
                )
                .with_trace_config(sdktrace::config().with_resource(resource))
                .install_batch(runtime::Tokio)
                .into_diagnostic()
                .wrap_err("install otlp provider")?;

            let tracer = provider.tracer(service_name.to_string());
            let otel_layer = layer().with_tracer(tracer);
            registry = registry.with(otel_layer);
        }

        registry
            .with(tracing_subscriber::fmt::layer())
            .try_init()
            .into_diagnostic()
            .wrap_err("init tracing subscriber")?;
        return Ok(());
    }

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .into_diagnostic()
        .wrap_err("init tracing subscriber")?;
    Ok(())
}
