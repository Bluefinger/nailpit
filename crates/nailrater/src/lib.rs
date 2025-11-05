mod modes;

use core::future::{Ready, ready};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use actix_web::{
    Error, HttpMessage, HttpResponse,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{CONTENT_ENCODING, ContentType, HeaderMap},
    mime,
};
use futures_lite::{
    FutureExt,
    future::{Boxed, BoxedLocal},
};

use modes::LimitModes;
use nailbox::boxed_future_within;
use nailconfig::RateLimitingConfig;
use nailip::IdentifiedPeer;
use nailspicy::{SpicyPayloadKind, SpicyPayloads};
use parking_lot::Mutex;
use rapidhash::quality::RandomState;
use scc::HashMap;
use tokio::time::sleep;
use tracing::instrument;
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

impl<S> Transform<S, ServiceRequest> for NailRaterLayer
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse;

    type Error = Error;

    type Transform = NailRater<S>;

    type InitError = ();

    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(NailRater::new(
            &self.config,
            self.spicy_payload.clone(),
            service,
        )))
    }
}

#[derive(Debug, Clone)]
pub struct NailRater<S> {
    peers: Arc<HashMap<IdentifiedPeer, Peer, RandomState>>,
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

    #[inline]
    #[cfg_attr(
        feature = "detailed_traces",
        tracing::instrument(level = "trace", skip_all)
    )]
    fn track_visiting_peer(
        &self,
        proxied: IdentifiedPeer,
        headers: &HeaderMap,
    ) -> (PeerState, Option<SpicyPayloadKind>) {
        let peer = self
            .peers
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
    fn prune(peers: Arc<HashMap<IdentifiedPeer, Peer, RandomState>>) -> Boxed<()> {
        boxed_future_within(async move || {
            tracing::trace!("PRUNING STARTED");

            peers
                .retain_async(|_, v| v.last_seen.elapsed() < crate::PEER_TIMEOUT)
                .await
        })
    }
}

impl<S> Service<ServiceRequest> for NailRater<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse;

    type Error = Error;

    type Future = Instrumented<BoxedLocal<Result<Self::Response, Self::Error>>>;

    actix_web::dev::forward_ready!(inner);

    #[instrument(name = "rate limiter", skip_all)]
    fn call(&self, req: ServiceRequest) -> Self::Future {
        let Some(proxied) = req.extensions().get::<IdentifiedPeer>().cloned() else {
            return ready(Ok(
                req.into_response(HttpResponse::InternalServerError().finish())
            ))
            .boxed_local()
            .in_current_span();
        };

        let (peer_state, supports_spicy) = self.track_visiting_peer(proxied, req.headers());

        let delay = match peer_state {
            PeerState::Ready => None,
            PeerState::Delay(duration) => Some(Box::pin(sleep(duration))),
            PeerState::Drop => {
                return ready(Ok(
                    req.into_response(HttpResponse::TooManyRequests().finish())
                ))
                .boxed_local()
                .in_current_span();
            }
            PeerState::SpicyDrop => {
                if let Some((payload, kind)) =
                    self.spicy_payload.as_deref().zip(supports_spicy).and_then(
                        |(payloads, kind)| {
                            payloads.get(&kind).map(|payload| (payload.clone(), kind))
                        },
                    )
                {
                    return ready(Ok(req.into_response(
                        HttpResponse::TooManyRequests()
                            .insert_header(ContentType(mime::TEXT_HTML_UTF_8))
                            .insert_header((CONTENT_ENCODING, kind.as_str()))
                            .body(payload),
                    )))
                    .boxed_local()
                    .in_current_span();
                } else {
                    return ready(Ok(
                        req.into_response(HttpResponse::TooManyRequests().finish())
                    ))
                    .boxed_local()
                    .in_current_span();
                }
            }
        };

        let prune = PRUNING_SCHEDULER.schedule(|| Self::prune(self.peers.clone()));

        let inner = self.inner.call(req);

        async move {
            if let Some(prune) = prune {
                prune.await;
            }

            if let Some(delay) = delay {
                delay.await;
            }

            inner.await
        }
        .boxed_local()
        .in_current_span()
    }
}
