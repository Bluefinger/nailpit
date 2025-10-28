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
    let state = nailstate::ServerState::new(app.config, app.inputs, app.interner, app.templates);
    let listener = nailnet::get_tcp_socket(&state.config.server.socket_addr)?;
    let ip = listener.local_addr()?;

    tracing::info!(
        port = ip.port(),
        address = ip.ip().to_string(),
        "{} listening on {}",
        std::thread::current().name().unwrap(),
        ip
    );

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

    let config = nailconfig::get_configuration()?;

    let (inputs, interner) = nailpit::inputs::get_input_files(config.as_ref())?;

    let templates = nailpit::inputs::get_template_files(config.as_ref())?;

    let spicy = nailspicy::get_spicy_payload(config.as_ref());

    nailpit::runtime::start(
        App::new(config, inputs, interner, spicy, templates),
        spawn_axum_worker,
    )?;

    Ok(())
}
