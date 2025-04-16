#![warn(clippy::undocumented_unsafe_blocks)]

mod body_stream;
mod config;
mod fv_parser;
mod html_gen;
mod inputs;
mod markov;
mod peer;
mod rng;
mod routes;
mod shutdown;
mod state;

use std::{
    net::SocketAddr,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use axum::http::HeaderValue;
use color_eyre::Result;
use config::{NailConfig, get_configuration};
use futures_concurrency::future::{Race, TryJoin};
use hyper::{HeaderMap, header::CONTENT_TYPE};
use inputs::get_input_files;
use logforth::append;
use logforth::filter::EnvFilter;
use markov::MarkovGen;
use nailkov::interner::Interner;
use parking_lot::RwLock;
use routes::nail_app;
use scc::HashMap;
use shutdown::{shutdown_task, wait_for_shutdown};
use state::ServerState;
use tokio::time::interval_at;
use wyrand::RandomWyHashState;
static INDEX: &str = include_str!("../templates/warning.html");

const SOURCE_TIMEOUT: Duration = Duration::from_secs(60 * 2);

static GEN_HEADER: LazyLock<HeaderMap> = LazyLock::new(|| {
    let mut headers = HeaderMap::new();
    headers.append(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers
});

static INTERNER: LazyLock<Arc<RwLock<Interner>>> = LazyLock::new(Default::default);

#[fastrace::trace]
async fn nailpit_cleanup(state: ServerState) {
    let tick_interval = Duration::from_secs(60 * 5);
    let mut tick = interval_at((Instant::now() + tick_interval).into(), tick_interval);
    loop {
        tick.tick().await;

        if state.sources.len() > 64 {
            state
                .sources
                .retain_async(|_, v| v.last_seen.elapsed() >= SOURCE_TIMEOUT)
                .await;

            log::info!("pit cleaned of corpses");
        }
    }
}

async fn spawn_axum_server<F>(
    state: ServerState,
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
            nail_app(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .into_future(),
    )
    .await??;

    Ok(())
}

async fn nailpit_main(config: Arc<NailConfig>, inputs: Arc<[MarkovGen]>) -> Result<()> {
    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());
    let shutdown_notifier = Arc::new(shutdown_notifier);
    let state = ServerState::new(
        Arc::new(HashMap::with_capacity_and_hasher(
            128,
            RandomWyHashState::new(),
        )),
        config,
        inputs,
    );

    tokio::spawn(
        (
            wait_for_shutdown(shutdown_notifier.clone()),
            nailpit_cleanup(state.clone()),
        )
            .race(),
    );

    (
        spawn_axum_server(
            state,
            wait_for_shutdown(shutdown_notifier.clone()),
        ),
        shutdown_task(shutdown_signal),
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

    let config: NailConfig = get_configuration()?;

    log::info!("Loaded config: {:?}", config);

    let inputs = get_input_files(&config)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(std::thread::available_parallelism()?.get().min(4))
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
