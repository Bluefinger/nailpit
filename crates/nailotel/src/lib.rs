use std::sync::{Arc, LazyLock};

use color_eyre::Result;

use nailconfig::NailConfig;
use opentelemetry::{KeyValue, trace::TracerProvider};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, Protocol, SpanExporter, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
};
use opentelemetry_semantic_conventions::{SCHEMA_URL, resource::SERVICE_VERSION};
use tracing::level_filters::LevelFilter;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt};

static RESOURCE: LazyLock<Resource> = LazyLock::new(resource);

fn resource() -> Resource {
    Resource::builder()
        .with_service_name(env!("CARGO_PKG_NAME"))
        .with_schema_url(
            [KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION"))],
            SCHEMA_URL,
        )
        .build()
}

pub fn init_logging_reporter(config: &NailConfig) -> Result<SdkLoggerProvider> {
    let log_exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint(&config.open_telemetry.endpoint)
        .with_compression(opentelemetry_otlp::Compression::Zstd)
        .with_protocol(Protocol::Grpc)
        .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
        .build()?;

    Ok(SdkLoggerProvider::builder()
        .with_batch_exporter(log_exporter)
        .with_resource(RESOURCE.clone())
        .build())
}

pub fn init_tracing_reporter(config: &NailConfig) -> Result<SdkTracerProvider> {
    let trace_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.open_telemetry.endpoint)
        .with_protocol(Protocol::Grpc)
        .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
        .with_compression(opentelemetry_otlp::Compression::Zstd)
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(trace_exporter)
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            1.0,
        ))))
        .with_resource(RESOURCE.clone())
        .with_id_generator(RandomIdGenerator::default())
        .build();

    Ok(provider)
}

pub fn init_telemetry(
    config: Arc<nailconfig::NailConfig>,
) -> Result<(Option<SdkLoggerProvider>, Option<SdkTracerProvider>)> {
    let otel_logger = if config.open_telemetry.logs {
        Some(init_logging_reporter(config.as_ref())?)
    } else {
        None
    };

    let otel_traces = if config.open_telemetry.traces {
        Some(init_tracing_reporter(config.as_ref())?)
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_level(true)
                .with_thread_names(true)
                .with_writer(std::io::stdout)
                .json(),
        )
        .with(otel_logger.as_ref().map(|otel| {
            let filter_otel = EnvFilter::new("info")
                .add_directive("hyper=off".parse().unwrap())
                .add_directive("opentelemetry=off".parse().unwrap())
                .add_directive("tonic=off".parse().unwrap())
                .add_directive("h2=off".parse().unwrap())
                .add_directive("reqwest=off".parse().unwrap());

            OpenTelemetryTracingBridge::new(otel).with_filter(filter_otel)
        }))
        .with(
            otel_traces
                .as_ref()
                .map(|otel| OpenTelemetryLayer::new(otel.tracer("tracing-otel-subscriber"))),
        );

    tracing::info!("Welcome to Nailpit!");
    tracing::info!(configuration = ?config, "Loaded configuration");

    Ok((otel_logger, otel_traces))
}
