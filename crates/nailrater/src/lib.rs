mod futures;
mod modes;

use std::{
    net::IpAddr,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use axum::{body::Body, extract::Request, response::Response};
use fastrace::{
    Span,
    future::{FutureExt as SpanFutureExt, InSpan},
};
use futures::NailedResponseFuture;
use futures_lite::future::Boxed;

use hyper::HeaderMap;
use modes::LimitModes;
use nailbox::boxed_future_within;
use nailconfig::RateLimitingConfig;
use nailip::IdentifiedPeer;
use nailspicy::{SpicyPayloadKind, SpicyPayloads};
use parking_lot::RwLock;
use rapidhash::fast::RandomState;
use scc::HashMap;
use tokio::time::sleep;

const SOURCE_TIMEOUT: Duration = Duration::from_secs(60 * 2);

static PRUNING_SCHEDULE: RwLock<OnceLock<Instant>> = RwLock::new(OnceLock::new());

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum PeerState {
    #[default]
    Ready,
    Delay(Duration),
    Drop,
    SpicyDrop,
}

#[derive(Debug, Clone)]
struct Peer {
    count: u64,
    state: PeerState,
    last_seen: Instant,
    supports_spicy: Option<SpicyPayloadKind>,
}

#[derive(Debug, Clone)]
pub struct NailRaterLayer {
    config: RateLimitingConfig,
    spicy_payload: Option<Arc<SpicyPayloads>>,
}

impl NailRaterLayer {
    pub fn new(config: RateLimitingConfig, spicy_payload: Option<Arc<SpicyPayloads>>) -> Self {
        Self {
            config,
            spicy_payload,
        }
    }
}

impl<S> tower::Layer<S> for NailRaterLayer {
    type Service = NailRater<S>;

    fn layer(&self, inner: S) -> Self::Service {
        NailRater::new(&self.config, self.spicy_payload.clone(), inner)
    }
}

#[derive(Debug, Clone)]
pub struct NailRater<S> {
    peers: Arc<HashMap<IpAddr, Peer, RandomState>>,
    mode: LimitModes,
    spicy_payload: Option<Arc<SpicyPayloads>>,
    inner: S,
}

impl<S> NailRater<S> {
    pub fn new(
        mode: impl Into<LimitModes>,
        spicy_payload: Option<Arc<SpicyPayloads>>,
        inner: S,
    ) -> Self {
        Self {
            peers: Default::default(),
            mode: mode.into(),
            spicy_payload,
            inner,
        }
    }

    fn track_visiting_peer(
        &self,
        proxied: IpAddr,
        headers: &HeaderMap,
    ) -> (PeerState, Option<SpicyPayloadKind>) {
        let peer = self
            .peers
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
                supports_spicy: SpicyPayloadKind::accepts_encoding(headers),
            });

        (peer.state, peer.supports_spicy)
    }

    fn prune(peers: Arc<HashMap<IpAddr, Peer, RandomState>>) -> Boxed<()> {
        boxed_future_within(async move || {
            log::trace!("PRUNING STARTED");

            peers
                .retain_async(|_, v| v.last_seen.elapsed() < crate::SOURCE_TIMEOUT)
                .await
        })
    }

    fn prune_recorded_peers(&mut self) -> Option<Boxed<()>> {
        let mut lock = PRUNING_SCHEDULE.upgradable_read();
        let since = lock.get_or_init(Instant::now);

        let elapsed = since.elapsed();

        log::trace!("ELAPSED: {}ms", elapsed.as_millis());

        if elapsed >= crate::SOURCE_TIMEOUT {
            lock.with_upgraded(OnceLock::take);
            drop(lock);

            Some(Self::prune(self.peers.clone()))
        } else {
            None
        }
    }
}

impl<S, ReqBody> tower::Service<Request<ReqBody>> for NailRater<S>
where
    S: tower::Service<Request<ReqBody>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = InSpan<NailedResponseFuture<S::Future>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let parent = Span::enter_with_local_parent("NailRater");

        let Some(proxied) = req.extensions().get::<IdentifiedPeer>() else {
            return NailedResponseFuture::error().in_span(parent);
        };

        let (peer_state, supports_spicy) = self.track_visiting_peer(proxied.ip(), req.headers());

        let delay = match peer_state {
            PeerState::Ready => None,
            PeerState::Delay(delay) => Some(Box::pin(sleep(delay))),
            PeerState::SpicyDrop => {
                return self
                    .spicy_payload
                    .as_deref()
                    .zip(supports_spicy)
                    .and_then(|(payloads, kind)| {
                        payloads.get(&kind).map(|payload| (payload.clone(), kind))
                    })
                    .map_or_else(NailedResponseFuture::dropped, |(payload, kind)| {
                        NailedResponseFuture::spicy(payload, kind)
                    })
                    .in_span(parent);
            }
            _ => return NailedResponseFuture::dropped().in_span(parent),
        };

        let prune = self.prune_recorded_peers();

        let inner = self.inner.call(req);

        NailedResponseFuture::normal(prune, delay, inner).in_span(parent)
    }
}
