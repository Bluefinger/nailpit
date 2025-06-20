use std::sync::LazyLock;

use axum::{Router, extract::NestedPath, http::HeaderValue, response::Html, routing::get};
use fastrace::Span;
use fastrace_futures::StreamExt;
use hyper::{HeaderMap, StatusCode, header::CONTENT_TYPE};
use nailip::identify_peer;
use nailrater::NailRaterLayer;
use nailstate::{AppConfig, NailInputs, ServerState};
use nailstream::NailStream;
use nailtrace::tracing_root_span;
use tower::ServiceBuilder;
use tower_http::{
    CompressionLevel, ServiceBuilderExt, compression::CompressionLayer,
    normalize_path::NormalizePathLayer, request_id::MakeRequestUuid,
};

static INDEX: &str = include_str!("../../../templates/warning.html");

static GEN_HEADER: LazyLock<HeaderMap> = LazyLock::new(|| {
    let mut headers = HeaderMap::new();
    headers.append(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers
});

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

pub fn nail_app(routes: Router, state: ServerState) -> Router {
    routes
        .layer(
            ServiceBuilder::new()
                .set_x_request_id(MakeRequestUuid)
                .layer(axum::middleware::from_fn(identify_peer))
                .layer(axum::middleware::from_fn(tracing_root_span))
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
