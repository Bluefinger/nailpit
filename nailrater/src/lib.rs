mod futures;
mod maybe_headers;
mod modes;

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Request},
    response::Response,
};
use futures::NailedResponseFuture;
use futures_lite::{FutureExt, future::Boxed};
use hyper::HeaderMap;
use maybe_headers::{maybe_forwarded, maybe_x_forwarded_for, maybe_x_real_ip};
use modes::LimitModes;
use nailconfig::RateLimitingConfig;
use scc::HashMap;
use tokio::time::sleep;
use wyrand::RandomWyHashState;

const SOURCE_TIMEOUT: Duration = Duration::from_secs(60 * 2);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum PeerState {
    #[default]
    Ready,
    Delay(Duration),
    Drop,
}

#[derive(Debug, Clone)]
struct Peer {
    count: u64,
    state: PeerState,
    last_seen: Instant,
}

#[derive(Debug, Clone)]
pub struct NailRaterLayer {
    config: RateLimitingConfig,
}

impl NailRaterLayer {
    pub fn new(config: RateLimitingConfig) -> Self {
        Self { config }
    }
}

impl<S> tower::Layer<S> for NailRaterLayer {
    type Service = NailRater<S>;

    fn layer(&self, inner: S) -> Self::Service {
        NailRater::new(&self.config, inner)
    }
}

#[derive(Debug, Clone)]
pub struct NailRater<S> {
    peers: Arc<HashMap<IpAddr, Peer, RandomWyHashState>>,
    mode: LimitModes,
    schedule_pruning: Option<Instant>,
    inner: S,
}

impl<S> NailRater<S> {
    pub fn new(mode: impl Into<LimitModes>, inner: S) -> Self {
        Self {
            peers: Default::default(),
            mode: mode.into(),
            schedule_pruning: None,
            inner,
        }
    }

    fn extract(
        headers: &HeaderMap,
        connection: Option<&ConnectInfo<SocketAddr>>,
    ) -> Option<IpAddr> {
        maybe_x_forwarded_for(headers)
            .or_else(|| maybe_x_real_ip(headers))
            .or_else(|| maybe_forwarded(headers))
            .or_else(|| connection.map(|connect_info| connect_info.ip()))
    }

    fn track_visiting_peer(&self, proxied: IpAddr) -> PeerState {
        self.peers
            .entry(proxied)
            .and_modify(|p| {
                p.count += 1;
                p.last_seen = Instant::now();
                p.state = self.mode.limit(&p.count);
            })
            .or_insert_with(|| Peer {
                count: 1,
                state: self.mode.limit(&1),
                last_seen: Instant::now(),
            })
            .state
    }

    fn prune(peers: Arc<HashMap<IpAddr, Peer, RandomWyHashState>>) -> Boxed<()> {
        async move {
            peers
                .retain_async(|_, v| v.last_seen.elapsed() < crate::SOURCE_TIMEOUT)
                .await
        }
        .boxed()
    }

    fn prune_recorded_peers(&mut self) -> Option<Boxed<()>> {
        if self.schedule_pruning.is_none() {
            self.schedule_pruning.replace(Instant::now());
        }

        self.schedule_pruning
            .take_if(|since| since.elapsed() >= crate::SOURCE_TIMEOUT)
            .map(|_| Self::prune(self.peers.clone()))
    }
}

impl<S, ReqBody> tower::Service<Request<ReqBody>> for NailRater<S>
where
    S: tower::Service<Request<ReqBody>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = NailedResponseFuture<S::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let Some(proxied) = Self::extract(
            req.headers(),
            req.extensions().get::<ConnectInfo<SocketAddr>>(),
        ) else {
            return NailedResponseFuture::error();
        };

        let peer = self.track_visiting_peer(proxied);

        let delay = match peer {
            PeerState::Ready => None,
            PeerState::Delay(delay) => Some(Box::pin(sleep(delay))),
            PeerState::Drop => return NailedResponseFuture::dropped(),
        };

        let prune = self.prune_recorded_peers();

        let inner = self.inner.call(req);

        NailedResponseFuture::normal(prune, delay, inner)
    }
}
