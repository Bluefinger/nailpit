use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, http::header::ContentType, web::ThinData};
use nailrng::FastRng;
use nailspicy::SpicyPayloads;
use nailstate::{AppConfig, ServerState};
use nailtrace::middleware::TracingLogger;
use tracing_futures::Instrument;

#[tracing::instrument(skip_all)]
async fn warning_index(ThinData(state): ThinData<ServerState>, req: HttpRequest) -> HttpResponse {
    let mut rng = FastRng::default();

    let stream = state
        .inputs
        .get_random_input(&mut rng)
        .into_stream(
            req.match_pattern().unwrap(),
            state.config.clone_inner(),
            state.inputs.get_interner(),
            state.inputs.get_warning_template(),
            rng,
        )
        .in_current_span();

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .streaming(stream)
}

#[tracing::instrument(skip_all)]
async fn generated_page(ThinData(state): ThinData<ServerState>, req: HttpRequest) -> HttpResponse {
    let mut rng = FastRng::default();

    let stream = state
        .inputs
        .get_random_input(&mut rng)
        .into_stream(
            req.match_pattern().unwrap(),
            state.config.clone_inner(),
            state.inputs.get_interner(),
            state.inputs.get_generated_template(),
            rng,
        )
        .in_current_span();

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .streaming(stream)
}

pub fn nail_web_app_config(
    config: &AppConfig,
    spicy: &Option<Arc<SpicyPayloads>>,
    cfg: &mut actix_web::web::ServiceConfig,
) {
    config.server.pit_routes.iter().for_each(|path| {
        let traces = TracingLogger::new(if config.open_telemetry.traces {
            nailtrace::middleware::BuilderKind::Default
        } else {
            nailtrace::middleware::BuilderKind::Minimal
        });

        cfg.service(
            actix_web::web::scope(path)
                .wrap(nailrater::NailRaterLayer::new(
                    config.rate_limiting.clone(),
                    spicy.clone(),
                ))
                .wrap(traces)
                .route("/", actix_web::web::get().to(warning_index))
                .route("/{generated}", actix_web::web::get().to(generated_page)),
        );
    });
}
