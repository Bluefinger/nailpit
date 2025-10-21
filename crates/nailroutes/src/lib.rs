use std::sync::Arc;

use axum::{Router, extract::MatchedPath, response::IntoResponse, routing::get};
use fastrace::Span;
use fastrace_futures::StreamExt;
use hyper::StatusCode;
use nailgen::{GeneratedTemplate, Template, WarningTemplate};
use nailip::identify_peer;
use nailrater::NailRaterLayer;
use nailrng::FastRng;
use nailspicy::SpicyPayloads;
use nailstate::{AppConfig, NailInputs, ServerState};
use nailstream::NailResponseStream;
use nailtrace::tracing_root_span;
use tower::ServiceBuilder;
use tower_http::{
    CompressionLevel, ServiceBuilderExt,
    compression::{CompressionLayer, DefaultPredicate, Predicate, predicate::SizeAbove},
    normalize_path::NormalizePathLayer,
    request_id::MakeRequestUuid,
};

#[fastrace::trace]
async fn warning(
    config: AppConfig,
    inputs: NailInputs,
    matched: MatchedPath,
    generated_template: WarningTemplate,
) -> impl IntoResponse {
    let mut rng = FastRng::default();

    NailResponseStream::from_stream(
        inputs
            .get_random_input(&mut rng)
            .into_stream(
                matched,
                config.clone_inner(),
                inputs.get_interner(),
                Template::from(generated_template),
                rng,
            )
            .in_span(Span::enter_with_local_parent("Warning page stream")),
    )
}

#[fastrace::trace]
async fn generated(
    config: AppConfig,
    inputs: NailInputs,
    matched: MatchedPath,
    generated_template: GeneratedTemplate,
) -> impl IntoResponse {
    let mut rng = FastRng::default();

    NailResponseStream::from_stream(
        inputs
            .get_random_input(&mut rng)
            .into_stream(
                matched,
                config.clone_inner(),
                inputs.get_interner(),
                Template::from(generated_template),
                rng,
            )
            .in_span(Span::enter_with_local_parent("Generated stream")),
    )
}

pub fn nail_app(state: ServerState, spicy_payload: Option<Arc<SpicyPayloads>>) -> Router {
    let rate_limiting = state.config.rate_limiting.clone();

    nail_route(state)
        .layer(
            ServiceBuilder::new()
                .set_x_request_id(MakeRequestUuid)
                .layer(axum::middleware::from_fn(identify_peer))
                .layer(axum::middleware::from_fn(tracing_root_span))
                .layer(NormalizePathLayer::trim_trailing_slash())
                .layer(
                    CompressionLayer::new()
                        .quality(CompressionLevel::Default)
                        .compress_when(DefaultPredicate::new().and(SizeAbove::new(1024))),
                )
                .layer(NailRaterLayer::new(rate_limiting, spicy_payload))
                .propagate_x_request_id(),
        )
        .route("/favicon.ico", get(async || StatusCode::NOT_FOUND))
        .route("/health", get(async || StatusCode::NO_CONTENT))
}

pub fn nail_route(state: ServerState) -> Router {
    let generation_routes = Router::new()
        .route("/", get(warning))
        .route("/{*generated}", get(generated));

    state
        .config
        .server
        .pit_routes
        .iter()
        .fold(Router::new(), |router, path| {
            if path == "/" {
                router.merge(generation_routes.clone())
            } else {
                let nested_routes = generation_routes.clone();
                router.nest(path, nested_routes)
            }
        })
        .with_state(state)
}
