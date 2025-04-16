use std::time::Duration;

use axum::{
    BoxError, Router, error_handling::HandleErrorLayer, extract::NestedPath, response::Html,
    routing::get,
};
use hyper::StatusCode;
use nailstream::NailStream;
use tower::{ServiceBuilder, buffer::BufferLayer, limit::RateLimitLayer};
use tower_http::{compression::CompressionLayer, normalize_path::NormalizePathLayer};

use crate::{
    GEN_HEADER, INDEX,
    state::{AppConfig, NailInputs, ServerState, track_incoming_sources},
};

#[fastrace::trace]
async fn handler() -> Html<&'static str> {
    Html(INDEX)
}

#[fastrace::trace]
async fn generated(config: AppConfig, input: NailInputs, path: NestedPath) -> NailStream {
    NailStream::from_stream(
        input
            .get_random_input()
            .into_stream(path, config.clone_inner()),
    )
    .headers(GEN_HEADER.clone())
}

pub fn nail_app(state: ServerState) -> Router {
    nail_route(state.clone())
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
                .layer(RateLimitLayer::new(1000, Duration::from_secs(60)))
                .layer(NormalizePathLayer::trim_trailing_slash())
                .layer(axum::middleware::from_fn_with_state(
                    state,
                    track_incoming_sources,
                ))
                .layer(CompressionLayer::new().quality(tower_http::CompressionLevel::Default)),
        )
        .route("/health", get(async || StatusCode::NO_CONTENT))
}

pub fn nail_route(state: ServerState) -> Router {
    let index = Router::new().route("/", get(handler));

    let pit = state
        .config
        .pit_routes
        .iter()
        .fold(Router::new(), |router, path| {
            router.nest(path.as_str(), Router::new().fallback(get(generated)))
        })
        .with_state(state);

    index.merge(pit)
}
