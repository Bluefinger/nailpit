#![warn(clippy::undocumented_unsafe_blocks)]

mod body_stream;
mod fv_parser;
mod html_gen;
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
use futures_concurrency::future::{Race, TryJoin};
use hyper::{HeaderMap, header::CONTENT_TYPE};
use logforth::append;
use logforth::filter::EnvFilter;
use markov::MarkovGen;
use nailkov::interner::Interner;
use parking_lot::RwLock;
use routes::{nail_app, nail_health};
use shutdown::{shutdown_task, wait_for_shutdown};
use state::ServerState;
use tokio::time::interval_at;
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
    LazyLock::new(|| MarkovGen::new(256, "./input/markov.txt").unwrap());

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

async fn nailpit_axum(
    state: ServerState,
    shutdown_notifier: Arc<tokio::sync::watch::Sender<()>>,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    let health_listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await?;

    log::info!(
        "listening on http://{} & http://{}/health",
        listener.local_addr()?,
        health_listener.local_addr()?
    );

    let app = nail_app(state);

    let generator_app = async {
        tokio::spawn(
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(wait_for_shutdown(shutdown_notifier.clone()))
            .into_future(),
        )
        .await?
    };

    let health_app = async {
        tokio::spawn(
            axum::serve(health_listener, nail_health())
                .with_graceful_shutdown(wait_for_shutdown(shutdown_notifier.clone()))
                .into_future(),
        )
        .await?
    };

    (generator_app, health_app).try_join().await?;

    Ok(())
}

async fn nailpit_main() -> Result<()> {
    let (shutdown_notifier, shutdown_signal) = tokio::sync::watch::channel(());
    let shutdown_notifier = Arc::new(shutdown_notifier);
    let state = ServerState::default();

    tokio::spawn(
        (
            wait_for_shutdown(shutdown_notifier.clone()),
            nailpit_cleanup(state.clone()),
        )
            .race(),
    );

    (
        nailpit_axum(state, shutdown_notifier),
        shutdown_task(shutdown_signal),
    )
        .race()
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

    LazyLock::force(&MARKOV);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(std::thread::available_parallelism()?.get().min(4))
        .enable_all()
        .build()?;

    rt.block_on(nailpit_main())?;

    log::info!("Waiting for background tasks to complete...");

    // Wait at most 30 seconds for remaining background tasks to complete
    rt.shutdown_timeout(Duration::from_secs(60));

    log::info!("Everything shutdown gracefully. Good night :)");

    fastrace::flush();

    Ok(())
}
