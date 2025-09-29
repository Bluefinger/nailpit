#![forbid(unsafe_code)]
use std::{net::SocketAddr, sync::Arc};

use color_eyre::Result;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

async fn spawn_axum_server<F>(
    state: nailstate::ServerState,
    spicy: Option<nailspicy::SpicyPayloads>,
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

async fn nailpit_main(
    config: Arc<nailconfig::NailConfig>,
    inputs: Arc<[nailgen::MarkovGen]>,
    spicy: Option<nailspicy::SpicyPayloads>,
    shutdown_notifier: Arc<tokio::sync::watch::Sender<()>>,
) {
    let state = nailstate::ServerState::new(config, inputs);

    if let Err(e) = spawn_axum_server(
        state,
        spicy,
        nailpit::shutdown::wait_for_shutdown(shutdown_notifier),
    )
    .await
    {
        log::error!("Server failed with: {}", e);
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let config: nailconfig::NailConfig = nailconfig::get_configuration()?;

    let inputs = nailpit::inputs::get_input_files(&config)?;

    let spicy = nailspicy::get_spicy_payload(&config);

    let config = Arc::new(config);

    nailpit::runtime::start_tokio(config.clone(), move |shutdown| {
        nailpit_main(config.clone(), inputs.clone(), spicy.clone(), shutdown)
    });

    log::info!("Everything shutdown gracefully. Good night :)");

    Ok(())
}
