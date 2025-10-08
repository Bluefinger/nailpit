use std::{borrow::Cow, sync::Arc};

use fastrace::collector::Config;
use fastrace_opentelemetry::OpenTelemetryReporter;
use logforth::append::opentelemetry::OpentelemetryLogBuilder;
use nailconfig::NailConfig;
use opentelemetry::{InstrumentationScope, trace::SpanKind};
use opentelemetry_otlp::{LogExporter, Protocol, SpanExporter, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::Resource;

pub fn init_logging_reporter(config: &NailConfig) -> logforth::append::OpentelemetryLog {
    let log_exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint(&config.open_telemetry.endpoint)
        .build()
        .unwrap();

    let builder =
        OpentelemetryLogBuilder::new(config.open_telemetry.service_name.to_owned(), log_exporter);

    builder
        .label(
            "service.name",
            config.open_telemetry.service_name.to_owned(),
        )
        .build()
}

pub fn init_tracing_reporter(config: &NailConfig) {
    let reporter = OpenTelemetryReporter::new(
        SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&config.open_telemetry.endpoint)
            .with_protocol(Protocol::Grpc)
            .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
            .with_compression(opentelemetry_otlp::Compression::Zstd)
            .build()
            .expect("initialize oltp exporter"),
        SpanKind::Internal,
        Cow::Owned(
            Resource::builder()
                .with_service_name(config.open_telemetry.service_name.to_owned())
                .build(),
        ),
        InstrumentationScope::builder(config.open_telemetry.service_name.to_owned())
            .with_version(env!("CARGO_PKG_VERSION"))
            .build(),
    );

    fastrace::set_reporter(reporter, Config::default());
}

pub fn init_telemetry(config: Arc<nailconfig::NailConfig>) {
    logforth::builder()
        .dispatch(|d| {
            let d = d.filter(logforth::filter::EnvFilter::from_default_env());

            if config.open_telemetry.logs {
                d.diagnostic(logforth::diagnostic::FastraceDiagnostic::default())
                    .append(logforth::append::FastraceEvent::default())
                    .append(init_logging_reporter(config.as_ref()))
                    .append(logforth::append::Stderr::default())
            } else {
                d.append(logforth::append::Stderr::default())
            }
        })
        .apply();

    #[cfg(feature = "tracing")]
    if config.open_telemetry.traces {
        init_tracing_reporter(config.as_ref());
    }

    log::info!("Welcome to Nailpit!");
    log::info!("Loaded config: {config:?}");
}
