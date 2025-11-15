use axum::{
    Router,
    body::Bytes,
    extract::{MatchedPath, Request},
    http::HeaderValue,
    response::IntoResponse,
    routing::get,
};
use axum_extra::middleware::option_layer;
use hyper::StatusCode;
use nailrater::NailRaterLayer;
use nailrng::FastRng;
use nailstate::{AppConfig, NailInputs, ServerState};
use nailstream::NailResponseStream;
use nailtrace::trace_connection_layer;
use tower::ServiceBuilder;
use tower_http::{
    ServiceBuilderExt,
    normalize_path::NormalizePathLayer,
    request_id::{MakeRequestId, RequestId},
};
use tracing_futures::Instrument;
use uuid::Uuid;

/// A [`MakeRequestId`] that generates `UUID`s.
#[derive(Clone, Copy, Default)]
pub struct MakeRequestUuid;

impl MakeRequestId for MakeRequestUuid {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        // SAFETY: The UUID is converted to a valid UTF-8 string before being turned into
        // Bytes. As such, the Bytes instance corresponds to a valid internal repr for
        // HeaderValue, meaning we can skip validation directly.
        let request_id = unsafe {
            HeaderValue::from_maybe_shared_unchecked(Bytes::from(Uuid::now_v7().to_string()))
        };
        Some(RequestId::new(request_id))
    }
}

#[tracing::instrument(skip_all)]
async fn warning(config: AppConfig, inputs: NailInputs, matched: MatchedPath) -> impl IntoResponse {
    let mut rng = FastRng::default();

    NailResponseStream::from_stream(
        inputs
            .get_random_input(&mut rng)
            .into_stream(
                matched,
                config.clone_inner(),
                inputs.get_interner(),
                inputs.get_warning_template(),
                rng,
            )
            .in_current_span(),
    )
}

#[tracing::instrument(skip_all)]
async fn generated(
    config: AppConfig,
    inputs: NailInputs,
    matched: MatchedPath,
) -> impl IntoResponse {
    let mut rng = FastRng::default();

    NailResponseStream::from_stream(
        inputs
            .get_random_input(&mut rng)
            .into_stream(
                matched,
                config.clone_inner(),
                inputs.get_interner(),
                inputs.get_generated_template(),
                rng,
            )
            .in_current_span(),
    )
}

pub fn nail_app(state: ServerState) -> Router {
    let rate_limiting = state.config.rate_limiting.clone();
    let spicy_payload = state.spicy_payloads.get();
    let tracing_support = state.config.open_telemetry.traces;

    nail_route(state)
        .layer(
            ServiceBuilder::new()
                .set_x_request_id(MakeRequestUuid)
                .layer(option_layer(
                    tracing_support.then(|| axum::middleware::from_fn(trace_connection_layer)),
                ))
                .layer(NormalizePathLayer::trim_trailing_slash())
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
