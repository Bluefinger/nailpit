mod futures;
mod modes;

use std::{
    net::IpAddr,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use axum::{body::Body, extract::Request, response::Response};
use futures::NailedResponseFuture;
use futures_lite::future::Boxed;

use hyper::HeaderMap;
use modes::LimitModes;
use nailbox::boxed_future_within;
use nailconfig::RateLimitingConfig;
use nailip::IdentifiedPeer;
use nailspicy::{SpicyPayloadKind, SpicyPayloads};
use parking_lot::Mutex;
use rapidhash::quality::RandomState;
use scc::HashMap;
use tokio::time::sleep;
use tracing_futures::{Instrument, Instrumented};

const PEER_TIMEOUT: Duration = Duration::from_secs(60 * 2);

struct Scheduler {
    value: Mutex<Option<Instant>>,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    #[inline]
    fn schedule<F, T>(&self, task: F) -> Option<T>
    where
        F: Fn() -> T,
    {
        let mut inner = self.value.lock();
        let elapsed = inner.get_or_insert_with(Instant::now).elapsed();

        if elapsed >= PEER_TIMEOUT {
            inner.take();
            drop(inner);

            Some(task())
        } else {
            None
        }
    }
}

static PRUNING_SCHEDULER: Scheduler = Scheduler::new();

static PEERS: LazyLock<HashMap<IpAddr, Peer, RandomState>> = LazyLock::new(Default::default);

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

    #[inline]
    fn layer(&self, inner: S) -> Self::Service {
        NailRater::new(&self.config, self.spicy_payload.clone(), inner)
    }
}

#[derive(Debug, Clone)]
pub struct NailRater<S> {
    mode: LimitModes,
    spicy_payload: Option<Arc<SpicyPayloads>>,
    inner: S,
}

impl<S> NailRater<S> {
    #[inline]
    pub fn new(
        mode: impl Into<LimitModes>,
        spicy_payload: Option<Arc<SpicyPayloads>>,
        inner: S,
    ) -> Self {
        LazyLock::force(&PEERS);

        Self {
            mode: mode.into(),
            spicy_payload,
            inner,
        }
    }

    #[inline]
    #[cfg_attr(
        feature = "detailed_traces",
        tracing::instrument(level = "trace", skip_all)
    )]
    fn track_visiting_peer(
        &self,
        proxied: IpAddr,
        headers: &HeaderMap,
    ) -> (PeerState, Option<SpicyPayloadKind>) {
        let peer = PEERS
            .entry_sync(proxied)
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

    #[inline]
    #[cfg_attr(
        feature = "detailed_traces",
        tracing::instrument(name = "prune_old_peers", level = "trace", skip_all)
    )]
    fn prune() -> Boxed<()> {
        boxed_future_within(async move || {
            tracing::trace!("PRUNING STARTED");

            PEERS
                .retain_async(|_, v| v.last_seen.elapsed() < crate::PEER_TIMEOUT)
                .await
        })
    }
}

impl<S, ReqBody> tower::Service<Request<ReqBody>> for NailRater<S>
where
    S: tower::Service<Request<ReqBody>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Instrumented<NailedResponseFuture<S::Future>>;

    #[inline]
    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[tracing::instrument(name = "rate_limiter", skip_all)]
    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let Some(proxied) = req.extensions().get::<IdentifiedPeer>() else {
            return NailedResponseFuture::error().in_current_span();
        };

        let (peer_state, supports_spicy) = self.track_visiting_peer(proxied.ip(), req.headers());

        let delay = match peer_state {
            PeerState::Ready => None,
            PeerState::Delay(delay) => Some(boxed_future_within(|| sleep(delay))),
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
                    .in_current_span();
            }
            _ => return NailedResponseFuture::dropped().in_current_span(),
        };

        let prune = PRUNING_SCHEDULER.schedule(Self::prune);

        let inner = self.inner.call(req);

        NailedResponseFuture::normal(prune, delay, inner).in_current_span()
    }
}
