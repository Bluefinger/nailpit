use axum::{Router, extract::NestedPath, response::Html, routing::get};
use fastrace::Span;
use fastrace_futures::StreamExt;
use hyper::StatusCode;
use nailrater::NailRaterLayer;
use nailstream::NailStream;
use nailtrace::NailTraceLayer;
use tower::ServiceBuilder;
use tower_http::{
    CompressionLevel, ServiceBuilderExt, compression::CompressionLayer,
    normalize_path::NormalizePathLayer, request_id::MakeRequestUuid,
};

use crate::{
    GEN_HEADER, INDEX,
    state::{AppConfig, NailInputs, ServerState},
};

#[fastrace::trace]
async fn index() -> Html<&'static str> {
    Html(INDEX)
}

#[fastrace::trace]
async fn generated(config: AppConfig, input: NailInputs, path: NestedPath) -> NailStream {
    NailStream::from_stream(
        input
            .get_random_input()
            .into_stream(path, config.clone_inner())
            .in_span(Span::enter_with_local_parent("Nailstream")),
    )
    .headers(GEN_HEADER.clone())
}

pub fn nail_app(state: ServerState) -> Router {
    nail_route(state.clone())
        .layer(
            ServiceBuilder::new()
                .set_x_request_id(MakeRequestUuid)
                .layer(NailTraceLayer)
                .layer(NormalizePathLayer::trim_trailing_slash())
                .layer(CompressionLayer::new().quality(CompressionLevel::Default))
                .layer(NailRaterLayer::new(state.config.rate_limiting.clone()))
                .propagate_x_request_id(),
        )
        .route("/favicon.ico", get(async || StatusCode::NOT_FOUND))
        .route("/health", get(async || StatusCode::NO_CONTENT))
}

pub fn nail_route(state: ServerState) -> Router {
    let index = Router::new().route("/", get(index));

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
