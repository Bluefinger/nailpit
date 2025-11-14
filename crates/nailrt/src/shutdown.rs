use color_eyre::Result;
use futures_concurrency::future::Race;
use tokio::signal;
use tokio_util::sync::CancellationToken;

pub async fn shutdown_task(notifier: CancellationToken) -> Result<()> {
    tracing::info!("Listening for shutdown signals");

    let cancel = notifier.drop_guard();

    let sigterm = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())?
            .recv()
            .await;

        Ok(())
    };

    tokio::spawn((signal::ctrl_c(), sigterm).race()).await??;

    tracing::info!("Shutdown signal received, finishing...");

    drop(cancel);

    Ok(())
}
