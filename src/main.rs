#![forbid(unsafe_code)]
use color_eyre::Result;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

async fn spawn_axum_worker(
    state: nailstate::ServerState,
    shutdown_notifier: CancellationToken,
) -> Result<()> {
    let listener = nailnet::get_tcp_socket(&state.config.server.socket_addr)?;
    let ip = listener.local_addr()?;

    tracing::info!(
        port = ip.port(),
        address = ip.ip().to_string(),
        "{} listening on {}",
        std::thread::current().name().unwrap(),
        ip
    );

    tokio::spawn(nailserve::serve(
        listener,
        nailroutes::nail_app(state),
        shutdown_notifier,
    ))
    .await?
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let config = nailconfig::get_configuration()?;

    let (inputs, interner) = nailpit::inputs::get_input_files(config.as_ref())?;

    let templates = nailpit::inputs::get_template_files(config.as_ref())?;

    let spicy = nailspicy::get_spicy_payload(config.as_ref());

    nailrt::start(
        nailstate::ServerState::new(config, inputs, interner, templates, spicy),
        spawn_axum_worker,
    )?;

    Ok(())
}
