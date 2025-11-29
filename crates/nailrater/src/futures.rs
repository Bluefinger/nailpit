use core::{
    pin::Pin,
    task::{Context, Poll},
};
use std::{sync::Arc, time::Instant};

use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use futures_lite::{FutureExt, future::Boxed, ready};
use hyper::{
    StatusCode,
    header::{CONTENT_ENCODING, CONTENT_TYPE},
};
use nailbox::boxed_future_within;
use nailip::IdentifiedPeer;
use nailspicy::{SpicyPayloadKind, SpicyPayloads};
use pin_project_lite::pin_project;
use rapidhash::quality::RandomState;
use scc::hash_map::Entry;
use tokio::time::{Sleep, sleep};

use crate::{PEERS, PeerRecord, PeerState, modes::LimitModes, scheduler::PRUNING_SCHEDULER};

pin_project! {
    pub struct NailedResponseFuture<S, F> {
        #[pin]
        state: NailedState<S, F>,
    }
}

pin_project! {
    #[project = NailedStateProj]
    enum NailedState<S, F> {
        RatePeer {
            req: Option<Request>,
            spicy_payloads: Option<Arc<SpicyPayloads>>,
            mode: LimitModes,
            entry: Boxed<Entry<'static, IdentifiedPeer, PeerRecord, RandomState>>,
            inner: S,
        },
        Normal {
            #[pin]
            state: NailedNormalFuture<F>,
        },
        Other {
            state: NailedOtherState,
        }
    }
}

pin_project! {
    struct NailedNormalFuture<T> {
        #[pin]
        state: NormalState,
        #[pin]
        inner: T,
    }
}

pin_project! {
    #[project = NormalStateProj]
    enum NormalState {
        Prune {
            prune: Boxed<()>,
            delay: Option<Pin<Box<Sleep>>>,
        },
        Delay {
            delay: Pin<Box<Sleep>>,
        },
        Pass,
    }
}

impl NormalState {
    #[inline]
    fn new(prune: Option<Boxed<()>>, delay: Option<Pin<Box<Sleep>>>) -> Self {
        match (prune, delay) {
            (Some(prune), delay) => NormalState::Prune { prune, delay },
            (None, Some(delay)) => NormalState::Delay { delay },
            (None, None) => NormalState::Pass,
        }
    }
}

enum NailedOtherState {
    Dropped,
    Spicy {
        payload: Bytes,
        kind: SpicyPayloadKind,
    },
    Error,
    Finished,
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

impl<S, T> NailedResponseFuture<S, T> {
    #[inline]
    pub fn rate_peer(
        peer: IdentifiedPeer,
        spicy_payloads: Option<Arc<SpicyPayloads>>,
        mode: LimitModes,
        req: Request,
        inner: S,
    ) -> Self {
        Self {
            state: NailedState::RatePeer {
                req: Some(req),
                mode,
                spicy_payloads,
                entry: boxed_future_within(|| PEERS.entry_async(peer)),
                inner,
            },
        }
    }

    #[inline]
    pub fn error() -> Self {
        Self {
            state: NailedState::Other {
                state: NailedOtherState::Error,
            },
        }
    }
}

impl<E, F, S> Future for NailedResponseFuture<S, F>
where
    F: Future<Output = Result<Response<Body>, E>>,
    S: tower::Service<Request, Response = Response<Body>, Future = F> + Send + 'static,
    S::Future: Send + 'static,
{
    type Output = Result<Response<Body>, E>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.project().state;

        loop {
            match state.as_mut().project() {
                NailedStateProj::RatePeer {
                    req,
                    spicy_payloads,
                    mode,
                    entry,
                    inner,
                } => {
                    let entry = ready!(entry.poll(cx));
                    let req = req.take().unwrap(); // If this panics, it is because the future was polled twice in the wrong state.
                    let peer = entry
                        .and_modify(|p| {
                            p.count += 1;
                            p.last_seen = Instant::now();
                            p.state = mode.limit(&p.count);
                        })
                        .or_insert_with_key(|proxied| {
                            tracing::info!("remote.peer" = %proxied, "New remote peer");
                            PeerRecord {
                                count: 1,
                                state: mode.limit(&1),
                                last_seen: Instant::now(),
                                supports_spicy: SpicyPayloadKind::accepts_encoding(req.headers()),
                            }
                        });

                    let delay = match peer.state {
                        PeerState::Ready => None,
                        PeerState::Delay(delay) => Some(boxed_future_within(|| sleep(delay))),
                        PeerState::SpicyDrop => {
                            let new_state = spicy_payloads
                                .as_deref()
                                .zip(peer.supports_spicy)
                                .and_then(|(payloads, kind)| {
                                    payloads.get(&kind).map(|payload| (payload.clone(), kind))
                                })
                                .map_or_else(
                                    || NailedState::Other {
                                        state: NailedOtherState::Dropped,
                                    },
                                    |(payload, kind)| NailedState::Other {
                                        state: NailedOtherState::Spicy { payload, kind },
                                    },
                                );

                            state.set(new_state);

                            continue;
                        }
                        _ => {
                            state.set(NailedState::Other {
                                state: NailedOtherState::Dropped,
                            });

                            continue;
                        }
                    };

                    let prune = PRUNING_SCHEDULER.schedule(prune);

                    let inner = inner.call(req);

                    state.set(NailedState::Normal {
                        state: NailedNormalFuture {
                            state: NormalState::new(prune, delay),
                            inner,
                        },
                    });
                }
                NailedStateProj::Normal { state } => return state.poll(cx),
                NailedStateProj::Other { state } => return Poll::Ready(Ok(state.build_response())),
            }
        }
    }
}

impl<E, F> Future for NailedNormalFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match this.state.as_mut().project() {
                NormalStateProj::Prune { prune, delay } => {
                    ready!(prune.as_mut().poll(cx));

                    let next_state = delay
                        .take()
                        .map_or(NormalState::Pass, |delay| NormalState::Delay { delay });

                    this.state.set(next_state);
                }
                NormalStateProj::Delay { delay } => {
                    ready!(delay.as_mut().poll(cx));

                    this.state.set(NormalState::Pass);
                }
                NormalStateProj::Pass => return this.inner.poll(cx),
            }
        }
    }
}

impl NailedOtherState {
    #[cfg_attr(
        feature = "detailed_traces",
        tracing::instrument(name = "Other Response", level = "trace", skip_all)
    )]
    #[inline]
    fn build_response(&mut self) -> Response<Body> {
        let state = core::mem::replace(self, Self::Finished);

        match state {
            Self::Dropped => (StatusCode::TOO_MANY_REQUESTS, "Go away").into_response(),
            Self::Spicy { payload, kind } => {
                let mut response = (StatusCode::TOO_MANY_REQUESTS, payload).into_response();

                let headers = response.headers_mut();

                headers.insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                );
                headers.insert(CONTENT_ENCODING, HeaderValue::from_static(kind.as_str()));

                response
            }
            Self::Error => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something is broken here",
            )
                .into_response(),
            Self::Finished => unreachable!("Response future has been polled more than once"),
        }
    }
}
