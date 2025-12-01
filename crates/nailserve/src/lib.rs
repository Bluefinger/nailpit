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
    listener: TcpListener,
    app: Router,
    graceful: &GracefulShutdown,
) -> color_eyre::Result<()> {
    let server = Builder::new(TokioExecutor::new());

    loop {
        let (socket, remote_addr) = listener.accept().await?;

        let tower_service = app.clone();

        let socket = TokioIo::new(socket);

        let hyper_service = hyper::service::service_fn(move |mut request: Request<Incoming>| {
            let peer = IdentifiedPeer::extract(request.headers(), remote_addr);
            request.extensions_mut().insert(peer);
            tower_service.clone().oneshot(request)
        });

        let conn = server.serve_connection(socket, hyper_service);

        tokio::spawn(graceful.watch(conn.into_owned()));
    }
}

async fn cancel_loop(token: CancellationToken) -> color_eyre::Result<()> {
    token.cancelled().await;
    Ok(())
}

/// Serves an Axum [`Router`] app with `hyper`, taking a [`CancellationToken`] to do a graceful shutdown
/// loop.
pub async fn serve(
    listener: TcpListener,
    app: Router,
    token: CancellationToken,
) -> color_eyre::Result<()> {
    let graceful = GracefulShutdown::new();

    (server_loop(listener, app, &graceful), cancel_loop(token))
        .race()
        .await?;

    graceful.shutdown().await;

    Ok(())
}
