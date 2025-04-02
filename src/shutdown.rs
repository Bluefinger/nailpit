use std::sync::Arc;

use color_eyre::Result;
use futures_concurrency::future::Race;
use tokio::{signal, sync::watch::{Sender, Receiver}};

#[fastrace::trace]
pub async fn shutdown_task(notifier: Receiver<()>) -> Result<()> {
    log::info!("Listening for shutdown signals");

    let sigterm = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())?
            .recv()
            .await;

        Ok(())
    };

    (signal::ctrl_c(), sigterm).race().await?;

    log::info!("Shutdown signal received, finishing...");

    drop(notifier);

    Ok(())
}

pub async fn wait_for_shutdown(notifier: Arc<Sender<()>>) {
    notifier.closed().await;
}
