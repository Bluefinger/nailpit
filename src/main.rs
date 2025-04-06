mod body_stream;
mod markov;
mod pit;
mod rng;
mod shutdown;
mod state;

use std::{
    net::SocketAddr,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use axum::{
    BoxError, Router,
    error_handling::HandleErrorLayer,
    extract::ConnectInfo,
    http::HeaderValue,
    response::{Html, IntoResponse},
    routing::get,
};
use body_stream::BodyStream;
use color_eyre::Result;
use futures_concurrency::future::Race;
use hyper::{HeaderMap, StatusCode, header::CONTENT_TYPE};
use logforth::append;
use logforth::filter::EnvFilter;
use markov::MarkovGen;
use shutdown::{shutdown_task, wait_for_shutdown};
use state::{ServerState, track_incoming_sources};
use tokio::time::interval_at;
use tower::{ServiceBuilder, buffer::BufferLayer, limit::RateLimitLayer};

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

static MARKOV: LazyLock<MarkovGen> =
    LazyLock::new(|| MarkovGen::new(256, "./input/markov.txt").unwrap());

#[fastrace::trace]
async fn handler(source: ConnectInfo<SocketAddr>) -> Html<&'static str> {
    log::info!("Into the tarpit, {}", source.ip());

    Html(INDEX)
}

#[fastrace::trace]
async fn generated() -> impl IntoResponse {
    BodyStream::from_stream(MARKOV.clone().into_stream()).headers(GEN_HEADER.clone())
}

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

    log::info!("listening on http://{}", listener.local_addr()?);

    LazyLock::<MarkovGen>::force(&MARKOV);

    let app = Router::new()
        .route("/", get(handler))
        .fallback(get(generated))
        .layer(
            ServiceBuilder::new()
                .layer(fastrace_axum::FastraceLayer)
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Unhandled Error: {err}"),
                    )
                }))
                .layer(BufferLayer::new(1024))
                .layer(RateLimitLayer::new(1000, Duration::from_secs(60 * 5)))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    track_incoming_sources,
                )),
        )
        .with_state(state);

    tokio::spawn(
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(wait_for_shutdown(shutdown_notifier))
        .into_future(),
    )
    .await??;

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

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(std::thread::available_parallelism()?.get().min(4))
        .enable_all()
        .build()?;

    rt.block_on(nailpit_main())?;

    // Wait at most 30 seconds for remaining background tasks to complete
    rt.shutdown_timeout(Duration::from_secs(30));

    fastrace::flush();

    Ok(())
}
