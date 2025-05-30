use fastrace::collector::Config;
use fastrace_opentelemetry::OpenTelemetryReporter;
use logforth::append::opentelemetry::OpentelemetryLogBuilder;
use opentelemetry::{InstrumentationScope, trace::SpanKind};
use opentelemetry_otlp::{LogExporter, Protocol, SpanExporter, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::Resource;
use std::{borrow::Cow, time::Duration};

pub fn init_logging_reporter() -> logforth::append::OpentelemetryLog {
    let log_exporter = LogExporter::builder()
        .with_http()
        .with_endpoint("http://localhost:4317")
        .build()
        .unwrap();

    let builder = OpentelemetryLogBuilder::new("nailpit", log_exporter);

    builder.build().unwrap()
}

pub fn init_tracing_reporter() {
    // Initialize reporter
    let reporter = OpenTelemetryReporter::new(
        SpanExporter::builder()
            .with_tonic()
            .with_endpoint("http://127.0.0.1:4317")
            .with_protocol(Protocol::Grpc)
            .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
            .with_compression(opentelemetry_otlp::Compression::Zstd)
            .build()
            .expect("initialize oltp exporter"),
        SpanKind::Server,
        Cow::Owned(Resource::builder().with_service_name("nailpit").build()),
        InstrumentationScope::builder("nailpit")
            .with_version(env!("CARGO_PKG_VERSION"))
            .build(),
    );

    fastrace::set_reporter(
        reporter,
        Config::default().report_interval(Duration::from_millis(100)),
    );
}
