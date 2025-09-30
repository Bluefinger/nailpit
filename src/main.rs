#![forbid(unsafe_code)]
use std::{net::SocketAddr, sync::Arc};

use color_eyre::Result;
use nailpit::app::App;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

async fn spawn_axum_server<F>(
    state: nailstate::ServerState,
    spicy: Option<Arc<nailspicy::SpicyPayloads>>,
    shutdown: F,
) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let listener = nailpit::net::get_tcp_socket(&state.config.server.socket_addr)?;

    log::info!("worker listening on http://{}", listener.local_addr()?);

    tokio::spawn(
        axum::serve(
            listener,
            nailroutes::nail_app(nailroutes::nail_route(state.clone()), state, spicy)
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .into_future(),
    )
    .await??;

    Ok(())
}

async fn nailpit_worker_main(app: App, shutdown_notifier: Arc<tokio::sync::watch::Sender<()>>) {
    let state = nailstate::ServerState::new(app.config, app.inputs);

    if let Err(e) = spawn_axum_server(
        state,
        app.spicy,
        nailpit::shutdown::wait_for_shutdown(shutdown_notifier),
    )
    .await
    {
        log::error!("Server failed with: {}", e);
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let config = nailconfig::get_configuration().map(Arc::new)?;

    let inputs = nailpit::inputs::get_input_files(config.as_ref())?;

    let spicy = nailspicy::get_spicy_payload(config.as_ref()).map(Arc::new);

    nailpit::runtime::start(App::new(config, inputs, spicy), nailpit_worker_main)?;

    log::info!("Everything shutdown gracefully. Good night :)");

    Ok(())
}
