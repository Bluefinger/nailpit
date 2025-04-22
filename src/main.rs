#![forbid(unsafe_code)]
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use color_eyre::Result;
use futures_concurrency::future::{Race, TryJoin};
use logforth::append;
use logforth::filter::EnvFilter;
use mimalloc_safe::MiMalloc;
use tokio::time::interval_at;
use wyrand::RandomWyHashState;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[fastrace::trace]
async fn nailpit_cleanup(state: nailpit::state::ServerState) {
    let tick_interval = Duration::from_secs(60 * 5);
    let mut tick = interval_at((Instant::now() + tick_interval).into(), tick_interval);
    loop {
        tick.tick().await;

        if state.sources.len() > 64 {
            state
                .sources
                .retain_async(|_, v| v.last_seen.elapsed() >= nailpit::SOURCE_TIMEOUT)
                .await;

            log::info!("pit cleaned of corpses");
        }
    }
}

async fn spawn_axum_server<F>(state: nailpit::state::ServerState, shutdown: F) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind(&state.config.socket_addr).await?;

    log::info!("listening on http://{}", listener.local_addr()?,);

    tokio::spawn(
        axum::serve(
            listener,
            nailpit::routes::nail_app(state).into_make_service_with_connect_info::<SocketAddr>(),
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
) -> Result<()> {
    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());
    let shutdown_notifier = Arc::new(shutdown_notifier);
    let state = nailpit::state::ServerState::new(
        Arc::new(scc::HashMap::with_capacity_and_hasher(
            128,
            RandomWyHashState::new(),
        )),
        config,
        inputs,
    );

    tokio::spawn(
        (
            nailpit::shutdown::wait_for_shutdown(shutdown_notifier.clone()),
            nailpit_cleanup(state.clone()),
        )
            .race(),
    );

    (
        spawn_axum_server(
            state,
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

    logforth::builder()
        .dispatch(|d| {
            d.filter(EnvFilter::from_default_env())
                .diagnostic(logforth::diagnostic::FastraceDiagnostic::default())
                .append(logforth::append::FastraceEvent::default())
                .append(append::Stderr::default())
        })
        .apply();

    fastrace::set_reporter(
        fastrace::collector::ConsoleReporter,
        fastrace::collector::Config::default(),
    );

    log::info!("Welcome to Nailpit!");

    let config: nailconfig::NailConfig = nailconfig::get_configuration()?;

    log::info!("Loaded config: {:?}", config);

    let inputs = nailpit::inputs::get_input_files(&config)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(
            std::thread::available_parallelism()?
                .get()
                .min(config.worker_threads),
        )
        .enable_all()
        .build()?;

    rt.block_on(nailpit_main(Arc::new(config), inputs))?;

    log::info!("Waiting for background tasks to complete...");

    // Wait at most 30 seconds for remaining background tasks to complete
    rt.shutdown_timeout(Duration::from_secs(60));

    log::info!("Everything shutdown gracefully. Good night :)");

    fastrace::flush();

    Ok(())
}
