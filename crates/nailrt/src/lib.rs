mod shutdown;

use core::time::Duration;

use futures_concurrency::future::TryJoin;
use nailstate::ServerState;
use tokio_util::sync::CancellationToken;

async fn worker_task<Fut, F>(
    worker: usize,
    app: ServerState,
    shutdown: CancellationToken,
    main_fn: &F,
) -> color_eyre::Result<()>
where
    Fut: Future<Output = color_eyre::Result<()>>,
    F: Fn(ServerState, CancellationToken) -> Fut + Sync + Send,
{
    while let Err(e) = main_fn(app.clone(), shutdown.clone()).await {
        tracing::error!(error = %e, worker = worker, "Failed to start");
        // Wait a moment before trying again
        tokio::time::sleep(Duration::from_secs(1)).await;
        tracing::info!(worker = worker, "Restarting Worker...");
    }

    Ok(())
}

pub fn start<Fut, F>(app: ServerState, main_fn: F) -> color_eyre::Result<()>
where
    Fut: Future<Output = color_eyre::Result<()>>,
    F: Fn(ServerState, CancellationToken) -> Fut + Sync + Send,
{
    let workers = std::thread::available_parallelism()?.min(app.config.server.worker_threads);

    let shutdown_notifier = CancellationToken::new();

    let core_ids = core_affinity::get_core_ids().unwrap_or_default();

    let mut id: usize = 0;

    core_ids
        .get({
            let pin = id;
            id += 1;
            pin
        })
        .copied()
        .map(core_affinity::set_for_current);

    // Main worker MUST start, else we just error out.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    std::thread::scope(|s| {
        for num in 1..workers.get() {
            let cloned = &main_fn;
            let app = &app;
            let shutdown_notifier = &shutdown_notifier;
            let core_id = core_ids
                .get({
                    let pin = id;
                    id += 1;
                    pin
                })
                .copied();

            // If any worker threads fail to be created, the program will terminate. If the
            // runtime within the worker thread fails to be created, this will also terminate the
            // program, and the error will get logged.
            std::thread::Builder::new()
                .name(format!("worker {num}"))
                .spawn_scoped(s, move || {
                    core_id.map(core_affinity::set_for_current);

                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => {
                            rt.block_on(worker_task(
                                num,
                                app.clone(),
                                shutdown_notifier.clone(),
                                cloned,
                            ))
                            .ok();

                            rt.shutdown_timeout(Duration::from_secs(60));
                        }
                        Err(e) => {
                            tracing::error!(error = %e, worker = num, "Failed to start");
                            shutdown_notifier.cancel();
                        }
                    }
                })
                .inspect_err(|_| shutdown_notifier.cancel())?;
        }

        rt.block_on(async {
            let guard = nailotel::init_telemetry(app.config.clone_inner())?;

            (
                worker_task(0, app.clone(), shutdown_notifier.clone(), &main_fn),
                crate::shutdown::shutdown_task(shutdown_notifier.clone()),
            )
                .try_join()
                .await?;

            tracing::info!("Waiting for background tasks to complete...");

            rt.spawn_blocking(|| drop(guard));

            color_eyre::eyre::Ok(())
        })
    })?;

    rt.shutdown_timeout(Duration::from_secs(60));

    Ok(())
}
