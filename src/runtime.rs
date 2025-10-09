use core::time::Duration;
use std::sync::Arc;

use crate::app::App;

pub fn start<Fut, F>(app: App, main_fn: F) -> color_eyre::Result<()>
where
    Fut: Future<Output = color_eyre::Result<()>>,
    F: Fn(App, Arc<tokio::sync::watch::Sender<()>>) -> Fut + Clone + Sync + Send,
{
    let workers = std::thread::available_parallelism()?.min(app.config.server.worker_threads);

    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());

    let shutdown_notifier = Arc::new(shutdown_notifier);

    // Main worker MUST start, else we just error out.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    std::thread::scope(|s| {
        for num in 1..workers.get() {
            let cloned = &main_fn;
            let app = &app;
            let shutdown_notifier = &shutdown_notifier;

            // If any worker threads fail to be created, the program will terminate. If the
            // runtime within the worker thread fails to be created, this won't terminate the
            // program, but the error will get logged.
            std::thread::Builder::new()
                .name(format!("Nailpit worker {num}"))
                .spawn_scoped(s, move || {
                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => {
                            // If we hit an application error case, restart the worker
                            while let Err(e) =
                                rt.block_on(cloned(app.clone(), shutdown_notifier.clone()))
                            {
                                log::error!("Worker {num} failed with: {e}");
                                // Wait a moment before trying again
                                std::thread::sleep(Duration::from_secs(1));
                                log::info!("Restarting Worker {num}...");
                            }

                            rt.shutdown_timeout(Duration::from_secs(60));
                        }
                        Err(e) => log::error!("Worker {num} failed to start: {e}"),
                    }
                })?;
        }

        rt.block_on(async {
            nailotel::init_telemetry(app.config.clone());

            let handle = tokio::spawn(crate::shutdown::shutdown_task(shutdown_signal));

            // If we hit an application error case, restart the worker.
            while let Err(e) = main_fn(app.clone(), shutdown_notifier.clone()).await {
                log::error!("Worker 0 failed with {e}");
                // Wait a moment before trying again
                tokio::time::sleep(Duration::from_secs(1)).await;
                log::info!("Restarting Worker 0...");
            }

            handle.await?
        })
    })?;

    log::info!("Waiting for background tasks to complete...");

    rt.shutdown_timeout(Duration::from_secs(60));

    Ok(())
}
