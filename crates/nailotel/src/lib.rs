use std::{borrow::Cow, sync::Arc};

use color_eyre::Result;
use fastrace::collector::Config;
use fastrace_opentelemetry::OpenTelemetryReporter;
use logforth::{
    append::{
        OpentelemetryLog,
        opentelemetry::{MakeBodyLayout, OpentelemetryLogBuilder},
    },
    layout::{JsonLayout, TextLayout},
};
use nailconfig::NailConfig;
use opentelemetry::InstrumentationScope;
use opentelemetry_otlp::{LogExporter, Protocol, SpanExporter, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::Resource;

pub fn init_logging_reporter(config: &NailConfig) -> Result<OpentelemetryLog> {
    let log_exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint(&config.open_telemetry.endpoint)
        .with_compression(opentelemetry_otlp::Compression::Zstd)
        .with_protocol(Protocol::Grpc)
        .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
        .build()?;

    let builder =
        OpentelemetryLogBuilder::new(config.open_telemetry.service_name.to_owned(), log_exporter);

    Ok(builder
        .label(
            "service.name",
            config.open_telemetry.service_name.to_owned(),
        )
        .make_body(MakeBodyLayout::new(JsonLayout::default()))
        .build())
}

pub fn init_tracing_reporter(config: &NailConfig) -> Result<()> {
    let reporter = OpenTelemetryReporter::new(
        SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&config.open_telemetry.endpoint)
            .with_protocol(Protocol::Grpc)
            .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
            .with_compression(opentelemetry_otlp::Compression::Zstd)
            .build()?,
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

    Ok(())
}

pub fn init_telemetry(config: Arc<nailconfig::NailConfig>) -> Result<()> {
    let otel_logger = init_logging_reporter(config.as_ref())?;

    logforth::core::builder()
        .dispatch(|d| {
            let d = d.filter(
                logforth::filter::env_filter::EnvFilterBuilder::from_default_env_or("info").build(),
            );

            if config.open_telemetry.logs {
                d.diagnostic(logforth::diagnostic::FastraceDiagnostic::default())
                    .append(logforth::append::FastraceEvent::default())
                    .append(otel_logger)
                    .append(logforth::append::Stderr::default().with_layout(TextLayout::default()))
            } else {
                d.append(logforth::append::Stderr::default().with_layout(TextLayout::default()))
            }
        })
        .apply();

    #[cfg(feature = "tracing")]
    if config.open_telemetry.traces {
        init_tracing_reporter(config.as_ref())?;
    }

    log::info!("Welcome to Nailpit!");
    log::info!(configuration:? = config; "Loaded configuration");

    Ok(())
}
