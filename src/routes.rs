use std::time::Duration;

use axum::{
    BoxError, Router,
    error_handling::HandleErrorLayer,
    response::{Html, IntoResponse},
    routing::get,
};
use hyper::StatusCode;
use tower::{ServiceBuilder, buffer::BufferLayer, limit::RateLimitLayer};
use tower_http::normalize_path::NormalizePathLayer;

use crate::{
    GEN_HEADER, INDEX, MARKOV,
    body_stream::BodyStream,
    state::{ServerState, track_incoming_sources},
};

#[fastrace::trace]
async fn handler() -> Html<&'static str> {
    Html(INDEX)
}

#[fastrace::trace]
async fn generated() -> impl IntoResponse {
    BodyStream::from_stream(MARKOV.clone().into_stream()).headers(GEN_HEADER.clone())
}

pub fn nail_app(state: ServerState) -> Router {
    nail_route().layer(
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
            )),
    )
}

pub fn nail_route() -> Router {
    Router::new()
        .route("/", get(handler))
        .nest("/private", Router::new().fallback(get(generated)))
}
