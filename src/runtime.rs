use core::time::Duration;
use std::sync::Arc;

use futures_concurrency::future::Join;

use crate::app::App;

pub fn start<Fut, F>(app: App, main_fn: F) -> std::io::Result<()>
where
    Fut: Future<Output = ()>,
    F: Fn(App, Arc<tokio::sync::watch::Sender<()>>) -> Fut + Clone + Send + 'static,
{
    let workers = std::thread::available_parallelism()?.min(app.config.server.worker_threads);

    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());

    let shutdown_notifier = Arc::new(shutdown_notifier);

    // Main worker MUST start, else we just error out.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    for num in 1..workers.get() {
        let cloned = main_fn.clone();
        let app = app.clone();
        let shutdown_notifier = shutdown_notifier.clone();

        // If any worker threads fail to be created, the program will terminate. If the
        // runtime within the worker thread fails to be created, this won't terminate the
        // program, but the error will get logged.
        std::thread::Builder::new()
            .name(format!("Nailpit worker {num}"))
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => {
                        rt.block_on(cloned(app, shutdown_notifier));

                        rt.shutdown_timeout(Duration::from_secs(60));
                    }
                    Err(e) => log::error!("Worker {} failed to start: {}", num, e),
                }
            })?;
    }

    rt.block_on(async move {
        crate::telemetry::init_telemetry(app.config.clone());

        let app_fut = main_fn(app, shutdown_notifier);

        let sig_watch = async move {
            if let Err(e) = crate::shutdown::shutdown_task(shutdown_signal).await {
                log::error!("SIG error: {}", e);
            }
        };

        (sig_watch, app_fut).join().await;
    });

    log::info!("Waiting for background tasks to complete...");

    rt.shutdown_timeout(Duration::from_secs(60));

    Ok(())
}
