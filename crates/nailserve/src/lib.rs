use axum::{Router, extract::Request};
use futures_concurrency::future::Race;
use hyper::body::Incoming;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::{conn::auto::Builder, graceful::GracefulShutdown},
};
use nailip::IdentifiedPeer;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

async fn server_loop(
    listener: &TcpListener,
    app: &Router,
    server: &Builder<TokioExecutor>,
    graceful: &GracefulShutdown,
) -> color_eyre::Result<bool> {
    let (socket, remote_addr) = listener.accept().await?;

    let tower_service = app.clone();

    let socket = TokioIo::new(socket);

    let hyper_service = hyper::service::service_fn(move |mut request: Request<Incoming>| {
        let peer = IdentifiedPeer::extract(request.headers(), remote_addr);
        request.extensions_mut().insert(peer);
        tower_service.clone().oneshot(request)
    });

    let conn = server.serve_connection(socket, hyper_service);

    let conn = graceful.watch(conn.into_owned());

    tokio::spawn(conn);

    Ok(false)
}

async fn cancel_loop(shutdown: &CancellationToken) -> color_eyre::Result<bool> {
    shutdown.cancelled().await;
    Ok(true)
}

pub async fn serve(
    listener: TcpListener,
    app: Router,
    shutdown: CancellationToken,
) -> color_eyre::Result<()> {
    let server = Builder::new(TokioExecutor::new());
    let graceful = GracefulShutdown::new();

    loop {
        let cancelled = (
            server_loop(&listener, &app, &server, &graceful),
            cancel_loop(&shutdown),
        )
            .race()
            .await?;

        if cancelled {
            drop(listener);
            break;
        }
    }

    graceful.shutdown().await;

    Ok(())
}
