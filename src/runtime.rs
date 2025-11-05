use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use actix_web::HttpResponse;
use color_eyre::eyre::Ok;
use nailspicy::SpicyPayloads;
use nailstate::ServerState;

pub fn run(state: ServerState, spicy: Option<Arc<SpicyPayloads>>) -> color_eyre::Result<()> {
    let workers = std::thread::available_parallelism()?.min(state.config.server.worker_threads);

    // Gets info for core ids if it can, else returns an empty Vec
    let core_ids = core_affinity::get_core_ids().unwrap_or_default();

    let next_core_id = Arc::new(AtomicUsize::new(0));

    actix_web::rt::System::new().block_on(async move {
        let guard = nailotel::init_telemetry(state.config.clone_inner())?;

        let socket = state.config.server.socket_addr.clone();

        tracing::info!(
            "{} listening on {}",
            &state.config.open_telemetry.service_name,
            &socket
        );

        actix_web::HttpServer::new(move || {
            let pin = Arc::clone(&next_core_id).fetch_add(1, Ordering::AcqRel);

            // Accesses the Core Id if available and sets current thread to be pinned to that core,
            // else it just does nothing.
            core_ids
                .get(pin)
                .copied()
                .map(core_affinity::set_for_current);

            let app = actix_web::App::new().app_data(actix_web::web::ThinData(state.clone()));

            let app = if let Some(spicy) = &spicy {
                app.app_data(actix_web::web::Data::from(spicy.clone()))
            } else {
                app
            };

            app.wrap(actix_web::middleware::Compress::default())
                .wrap(actix_web::middleware::NormalizePath::trim())
                .configure(|cfg| nailroutes::nail_web_app_config(&state.config, &spicy, cfg))
                .route(
                    "/health",
                    actix_web::web::get().to(async || HttpResponse::NoContent().finish()),
                )
        })
        .workers(workers.get())
        .bind(socket)?
        .run()
        .await?;

        actix_web::web::block(|| guard.shutdown()).await?;

        Ok(())
    })?;

    Ok(())
}
