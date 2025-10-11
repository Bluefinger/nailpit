#![forbid(unsafe_code)]
use core::net::SocketAddr;
use std::sync::Arc;

use color_eyre::Result;
use nailpit::app::App;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

async fn spawn_axum_worker(
    app: App,
    shutdown_notifier: Arc<tokio::sync::watch::Sender<()>>,
) -> Result<()> {
    let state = nailstate::ServerState::new(app.config, app.inputs, app.interner);
    let listener = nailnet::get_tcp_socket(&state.config.server.socket_addr)?;

    log::info!(port:% = listener.local_addr()?; "Worker listening on");

    axum::serve(
        listener,
        nailroutes::nail_app(state, app.spicy).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(nailpit::shutdown::wait_for_shutdown(shutdown_notifier))
    .await?;

    Ok(())
}

fn main() -> Result<()> {
    color_eyre::install()?;
    logforth_bridge_log::try_setup()?;

    let config = nailconfig::get_configuration().map(Arc::new)?;

    let (inputs, interner) = nailpit::inputs::get_input_files(config.as_ref())?;

    let spicy = nailspicy::get_spicy_payload(config.as_ref()).map(Arc::new);

    nailpit::runtime::start(App::new(config, inputs, interner, spicy), spawn_axum_worker)?;

    Ok(())
}
