use core::{num::NonZero, time::Duration};
use std::sync::Arc;

use futures_concurrency::future::Join;
use logforth::{append, filter::EnvFilter};

fn init_telemetry(config: Arc<nailconfig::NailConfig>) {
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

pub fn start_tokio<Fut, F>(config: Arc<nailconfig::NailConfig>, app: F)
where
    Fut: Future<Output = ()>,
    F: Fn(Arc<tokio::sync::watch::Sender<()>>) -> Fut + Clone + Send + 'static,
{
    let workers = std::thread::available_parallelism()
        .expect("System parallelism is not available")
        .min(
            NonZero::new(config.server.worker_threads)
                .expect("There must be more than 0 workers defined"),
        );

    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());

    let shutdown_notifier = Arc::new(shutdown_notifier);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for _ in 1..workers.get() {
        let cloned = app.clone();
        let shutdown_notifier = shutdown_notifier.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(cloned(shutdown_notifier));

            rt.shutdown_timeout(Duration::from_secs(60));
        });
    }

    rt.block_on(async move {
        init_telemetry(config);

        let fut = app(shutdown_notifier);

        let sh = async move {
            if let Err(e) = crate::shutdown::shutdown_task(shutdown_signal).await {
                log::error!("SIG error: {}", e);
            }
        };

        (sh, fut).join().await;
    });

    log::info!("Waiting for background tasks to complete...");

    rt.shutdown_timeout(Duration::from_secs(60));
}
