mod futures;
mod modes;
mod scheduler;

use std::{
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use axum::{body::Body, extract::Request, response::Response};
use futures::NailedResponseFuture;

use modes::LimitModes;
use nailconfig::RateLimitingConfig;
use nailip::IdentifiedPeer;
use nailspicy::{SpicyPayloadKind, SpicyPayloads};
use rapidhash::quality::RandomState;
use scc::HashMap;
use tracing_futures::{Instrument, Instrumented};

const PEER_TIMEOUT: Duration = Duration::from_secs(60 * 2);

static PEERS: LazyLock<HashMap<IdentifiedPeer, PeerRecord, RandomState>> =
    LazyLock::new(Default::default);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum PeerState {
    #[default]
    Ready,
    Delay(Duration),
    Drop,
    SpicyDrop,
}

#[derive(Debug, Clone)]
struct PeerRecord {
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
}

impl<S> tower::Service<Request> for NailRater<S>
where
    S: tower::Service<Request, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Instrumented<NailedResponseFuture<S, S::Future>>;

    #[inline]
    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[tracing::instrument(name = "rate_limiter", skip_all)]
    fn call(&mut self, req: Request) -> Self::Future {
        let Some(proxied) = req.extensions().get::<IdentifiedPeer>().cloned() else {
            return NailedResponseFuture::error().in_current_span();
        };

        let cloned = self.inner.clone();
        let ready_inner = core::mem::replace(&mut self.inner, cloned);

        NailedResponseFuture::rate_peer(proxied, self.spicy_payload.clone(), self.mode, req, ready_inner)
            .in_current_span()
    }
}
