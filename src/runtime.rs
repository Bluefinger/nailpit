use core::time::Duration;
use std::sync::Arc;

use color_eyre::eyre::OptionExt;

use crate::app::App;

pub fn start<Fut, F>(app: App, main_fn: F) -> color_eyre::Result<()>
where
    Fut: Future<Output = color_eyre::Result<()>>,
    F: Fn(App, Arc<tokio::sync::watch::Sender<()>>) -> Fut + Clone + Sync + Send,
{
    let workers = std::thread::available_parallelism()?.min(app.config.server.worker_threads);

    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());

    let shutdown_notifier = Arc::new(shutdown_notifier);

    let mut core_ids = core_affinity::get_core_ids().ok_or_eyre("Failed to get CPU affinity")?;

    let worker_cores = core_ids.split_off(1);

    core_affinity::set_for_current(core_ids[0]);

    // Main worker MUST start, else we just error out.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    std::thread::scope(|s| {
        for (num, core_id) in (1..workers.get()).zip(worker_cores) {
            let cloned = &main_fn;
            let app = &app;
            let shutdown_notifier = &shutdown_notifier;

            // If any worker threads fail to be created, the program will terminate. If the
            // runtime within the worker thread fails to be created, this won't terminate the
            // program, but the error will get logged.
            std::thread::Builder::new()
                .name(format!("worker {num}"))
                .spawn_scoped(s, move || {
                    core_affinity::set_for_current(core_id);

                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => {
                            // If we hit an application error case, restart the worker
                            while let Err(e) =
                                rt.block_on(cloned(app.clone(), shutdown_notifier.clone()))
                            {
                                tracing::error!(error = %e, worker = num, "Failed to start");
                                // Wait a moment before trying again
                                std::thread::sleep(Duration::from_secs(1));
                                tracing::info!(worker = num, "Restarting Worker...");
                            }

                            rt.shutdown_timeout(Duration::from_secs(60));
                        }
                        Err(e) => tracing::error!(error = %e, worker = num, "Failed to start"),
                    }
                })?;
        }

        rt.block_on(async {
            let _guard = nailotel::init_telemetry(app.config.clone())?;

            let handle = tokio::spawn(crate::shutdown::shutdown_task(shutdown_signal));

            // If we hit an application error case, restart the worker.
            while let Err(e) = main_fn(app.clone(), shutdown_notifier.clone()).await {
                tracing::error!(error = %e, worker = 0, "Failed to start");
                // Wait a moment before trying again
                tokio::time::sleep(Duration::from_secs(1)).await;
                tracing::info!(worker = 0, "Restarting Worker...");
            }

            tracing::info!("Waiting for background tasks to complete...");

            handle.await?
        })
    })?;

    rt.shutdown_timeout(Duration::from_secs(60));

    Ok(())
}
