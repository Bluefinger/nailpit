#![warn(clippy::undocumented_unsafe_blocks)]

mod body_stream;
mod config;
mod fv_parser;
mod html_gen;
mod markov;
mod peer;
mod rng;
mod routes;
mod shutdown;
mod state;

use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::HeaderValue,
    response::Response,
    serve::{IncomingStream, Listener},
};
use color_eyre::Result;
use config::{NailConfig, get_configuration};
use futures_concurrency::future::{Race, TryJoin};
use hyper::{HeaderMap, header::CONTENT_TYPE};
use logforth::append;
use logforth::filter::EnvFilter;
use markov::MarkovGen;
use nailkov::interner::Interner;
use parking_lot::RwLock;
use routes::{nail_app, nail_health};
use scc::HashMap;
use shutdown::{shutdown_task, wait_for_shutdown};
use state::ServerState;
use tokio::time::interval_at;
use tower::Service;
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

static MARKOV: LazyLock<MarkovGen> =
    LazyLock::new(|| MarkovGen::new("./input/markov.txt").unwrap());

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

async fn spawn_axum_task<L, M, S, F>(listener: L, app: M, shutdown: F) -> Result<()>
where
    L: Listener,
    L::Addr: core::fmt::Debug,
    M: for<'a> Service<IncomingStream<'a, L>, Error = Infallible, Response = S> + Send + 'static,
    for<'a> <M as Service<IncomingStream<'a, L>>>::Future: Send,
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send,
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .into_future(),
    )
    .await??;

    Ok(())
}

async fn nailpit_main(config: Arc<NailConfig>) -> Result<()> {
    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());
    let shutdown_notifier = Arc::new(shutdown_notifier);
    let state = ServerState::new(
        Arc::new(HashMap::with_capacity_and_hasher(
            128,
            RandomWyHashState::new(),
        )),
        config,
    );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    let health_listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await?;

    log::info!(
        "listening on http://{} & http://{}/health",
        listener.local_addr()?,
        health_listener.local_addr()?
    );

    tokio::spawn(
        (
            wait_for_shutdown(shutdown_notifier.clone()),
            nailpit_cleanup(state.clone()),
        )
            .race(),
    );

    (
        spawn_axum_task(
            listener,
            nail_app(state).into_make_service_with_connect_info::<SocketAddr>(),
            wait_for_shutdown(shutdown_notifier.clone()),
        ),
        spawn_axum_task(
            health_listener,
            nail_health(),
            wait_for_shutdown(shutdown_notifier),
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

    LazyLock::force(&MARKOV);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(std::thread::available_parallelism()?.get().min(4))
        .enable_all()
        .build()?;

    rt.block_on(nailpit_main(Arc::new(config)))?;

    log::info!("Waiting for background tasks to complete...");

    // Wait at most 30 seconds for remaining background tasks to complete
    rt.shutdown_timeout(Duration::from_secs(60));

    log::info!("Everything shutdown gracefully. Good night :)");

    fastrace::flush();

    Ok(())
}
