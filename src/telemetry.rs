use std::sync::Arc;

use logforth::{append, filter::EnvFilter};

pub fn init_telemetry(config: Arc<nailconfig::NailConfig>) {
    logforth::builder()
        .dispatch(|d| {
            let d = d.filter(EnvFilter::from_default_env());

            if config.open_telemetry.logs {
                d.diagnostic(logforth::diagnostic::FastraceDiagnostic::default())
                    .append(logforth::append::FastraceEvent::default())
                    .append(nailotel::init_logging_reporter(config.as_ref()))
                    .append(append::Stderr::default())
            } else {
                d.append(append::Stderr::default())
            }
        })
        .apply();

    #[cfg(feature = "tracing")]
    if config.open_telemetry.traces {
        nailotel::init_tracing_reporter(config.as_ref());
    }

    log::info!("Welcome to Nailpit!");
    log::info!("Loaded config: {config:?}");
}
