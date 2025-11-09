use std::sync::{Arc, OnceLock};

use color_eyre::Result;

use nailconfig::NailConfig;
use opentelemetry::{
    KeyValue, global, propagation::TextMapCompositePropagator, trace::TracerProvider,
};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, Protocol, SpanExporter, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::{
    Resource,
    logs::{
        BatchConfig as BatchLogConfig, SdkLoggerProvider,
        log_processor_with_async_runtime::BatchLogProcessor,
    },
    propagation::{BaggagePropagator, TraceContextPropagator},
    runtime::TokioCurrentThread,
    trace::{
        BatchConfig as BatchTraceConfig, RandomIdGenerator, Sampler, SdkTracerProvider,
        span_processor_with_async_runtime::BatchSpanProcessor,
    },
};
use opentelemetry_semantic_conventions::{SCHEMA_URL, resource::SERVICE_VERSION};
use tracing::level_filters::LevelFilter;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

static RESOURCE: OnceLock<Resource> = OnceLock::new();

pub struct OtelGuard {
    otel_logger: Option<SdkLoggerProvider>,
    otel_traces: Option<SdkTracerProvider>,
}

impl OtelGuard {
    pub fn shutdown(self) {
        self.otel_logger
            .map(|logs_provider| logs_provider.shutdown());
        self.otel_traces
            .map(|trace_provider| trace_provider.shutdown());
    }
}

fn resource(config: &NailConfig) -> Resource {
    RESOURCE
        .get_or_init(|| {
            Resource::builder()
                .with_service_name(config.open_telemetry.service_name.clone())
                .with_schema_url(
                    [KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION"))],
                    SCHEMA_URL,
                )
                .build()
        })
        .clone()
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
        .with_log_processor(
            BatchLogProcessor::builder(log_exporter, TokioCurrentThread)
                .with_batch_config(BatchLogConfig::default())
                .build(),
        )
        .with_resource(resource(config))
        .build())
}

pub fn init_tracing_reporter(config: &NailConfig) -> Result<SdkTracerProvider> {
    let baggage_propagator = BaggagePropagator::new();
    let trace_context_propagator = TraceContextPropagator::new();
    let composite_propagator = TextMapCompositePropagator::new(vec![
        Box::new(baggage_propagator),
        Box::new(trace_context_propagator),
    ]);

    global::set_text_map_propagator(composite_propagator);

    let trace_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.open_telemetry.endpoint)
        .with_protocol(Protocol::Grpc)
        .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
        .with_compression(opentelemetry_otlp::Compression::Zstd)
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_span_processor(
            BatchSpanProcessor::builder(trace_exporter, TokioCurrentThread)
                .with_batch_config(BatchTraceConfig::default())
                .build(),
        )
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            1.0,
        ))))
        .with_resource(resource(config))
        .with_id_generator(RandomIdGenerator::default())
        .build();

    Ok(provider)
}

pub fn init_telemetry(config: Arc<nailconfig::NailConfig>) -> Result<OtelGuard> {
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

    #[cfg(feature = "tokio_console")]
    let console_layer = console_subscriber::spawn();

    let registry = tracing_subscriber::registry();

    #[cfg(feature = "tokio_console")]
    let registry = registry.with(console_layer);

    registry
        .with(
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("hyper=off".parse().unwrap())
                .add_directive("opentelemetry=off".parse().unwrap())
                .add_directive("tonic=off".parse().unwrap())
                .add_directive("tower=off".parse().unwrap())
                .add_directive("h2=off".parse().unwrap())
                .add_directive("mio=off".parse().unwrap())
                .add_directive("actix_http=off".parse().unwrap())
                .add_directive("actix_server=off".parse().unwrap())
                .add_directive("reqwest=off".parse().unwrap()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_level(true)
                .with_thread_names(true)
                .with_writer(std::io::stdout)
                .compact(),
        )
        .with(otel_logger.as_ref().map(|otel| {
            let filter_otel = EnvFilter::new("info")
                .add_directive("hyper=off".parse().unwrap())
                .add_directive("opentelemetry=off".parse().unwrap())
                .add_directive("tonic=off".parse().unwrap())
                .add_directive("tower=off".parse().unwrap())
                .add_directive("h2=off".parse().unwrap())
                .add_directive("mio=off".parse().unwrap())
                .add_directive("actix_http=off".parse().unwrap())
                .add_directive("actix_server=off".parse().unwrap())
                .add_directive("reqwest=off".parse().unwrap());

            OpenTelemetryTracingBridge::new(otel).with_filter(filter_otel)
        }))
        .with(otel_traces.as_ref().map(|otel| {
            OpenTelemetryLayer::new(otel.tracer(config.open_telemetry.service_name.clone()))
        }))
        .init();

    tracing::info!("Welcome to Nailpit!");
    tracing::info!(configuration = ?config, "Loaded configuration");

    Ok(OtelGuard {
        otel_logger,
        otel_traces,
    })
}
