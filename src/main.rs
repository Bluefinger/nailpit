#![forbid(unsafe_code)]
use std::{net::SocketAddr, sync::Arc, time::Duration};

use color_eyre::Result;
use futures_concurrency::future::TryJoin;
use logforth::{append, filter::EnvFilter};

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
    let listener = tokio::net::TcpListener::bind(&state.config.socket_addr).await?;

    log::info!("listening on http://{}", listener.local_addr()?,);

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
) -> Result<()> {
    logforth::builder()
        .dispatch(|d| {
            let d = d.filter(EnvFilter::from_default_env());

            if config.open_telemetry.logs {
                d.diagnostic(logforth::diagnostic::FastraceDiagnostic::default())
                    .append(logforth::append::FastraceEvent::default())
                    .append(nailotel::init_logging_reporter(config.as_ref()))
                    .append(append::Stderr::default())
            } else {
                d.append(append::Stderr::default())
            }
        })
        .apply();

    #[cfg(feature = "tracing")]
    if config.open_telemetry.traces {
        nailotel::init_tracing_reporter(config.as_ref());
    }

    log::info!("Welcome to Nailpit!");
    log::info!("Loaded config: {config:?}");

    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());
    let shutdown_notifier = Arc::new(shutdown_notifier);
    let state = nailstate::ServerState::new(config, inputs);

    (
        spawn_axum_server(
            state,
            spicy,
            nailpit::shutdown::wait_for_shutdown(shutdown_notifier),
        ),
        nailpit::shutdown::shutdown_task(shutdown_signal),
    )
        .try_join()
        .await?;

    Ok(())
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let config: nailconfig::NailConfig = nailconfig::get_configuration()?;

    let inputs = nailpit::inputs::get_input_files(&config)?;

    let spicy = nailspicy::get_spicy_payload(&config);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(
            std::thread::available_parallelism()?
                .get()
                .min(config.worker_threads),
        )
        .enable_all()
        .build()?;

    rt.block_on(nailpit_main(Arc::new(config), inputs, spicy))?;

    log::info!("Waiting for background tasks to complete...");

    // Wait at most 60 seconds for remaining background tasks to complete
    rt.shutdown_timeout(Duration::from_secs(60));

    log::info!("Everything shutdown gracefully. Good night :)");

    Ok(())
}
