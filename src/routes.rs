use std::time::Duration;

use axum::{
    BoxError, Router,
    error_handling::HandleErrorLayer,
    response::{Html, IntoResponse},
    routing::get,
};
use hyper::StatusCode;
use tower::{ServiceBuilder, buffer::BufferLayer, limit::RateLimitLayer};
use tower_http::{compression::CompressionLayer, normalize_path::NormalizePathLayer};

use crate::{
    GEN_HEADER, INDEX,
    body_stream::BodyStream,
    markov::MarkovGen,
    state::{AppConfig, ServerState, track_incoming_sources},
};

#[fastrace::trace]
async fn handler() -> Html<&'static str> {
    Html(INDEX)
}

#[fastrace::trace]
async fn generated(config: AppConfig, input: MarkovGen) -> impl IntoResponse {
    BodyStream::from_stream(input.into_stream(config)).headers(GEN_HEADER.clone())
}

pub fn nail_app(state: ServerState) -> Router {
    nail_route(state.clone()).layer(
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
}

pub fn nail_route(state: ServerState) -> Router {
    Router::new().route("/", get(handler)).nest(
        "/private",
        Router::new().fallback(get(generated)).with_state(state),
    )
}

pub fn nail_health() -> Router {
    Router::new().route("/health", get(async || StatusCode::NO_CONTENT))
}
