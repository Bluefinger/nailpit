use std::{
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::Arc,
    task::Poll,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Request},
    response::{IntoResponse, Response},
};
use futures_lite::{FutureExt, future::Boxed, ready};
use hyper::{HeaderMap, StatusCode, header::FORWARDED};
use nailfv::{Parser, extract_for};
use pin_project_lite::pin_project;
use scc::HashMap;
use tokio::time::{Sleep, sleep};
use wyrand::RandomWyHashState;

const X_REAL_IP: &str = "x-real-ip";
const X_FORWARDED_FOR: &str = "x-forwarded-for";
const SOURCE_TIMEOUT: Duration = Duration::from_secs(60 * 2);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum PeerState {
    #[default]
    Ready,
    Limited,
}

#[derive(Debug, Clone)]
struct Peer {
    count: u64,
    state: PeerState,
    last_seen: Instant,
}

#[derive(Debug, Clone)]
pub struct NailRaterLayer {
    limit: u64,
    delay: u64,
}

impl NailRaterLayer {
    pub fn new(limit: u64, delay: u64) -> Self {
        Self { limit, delay }
    }
}

impl<S> tower::Layer<S> for NailRaterLayer {
    type Service = NailRater<S>;

    fn layer(&self, inner: S) -> Self::Service {
        NailRater::new(self.limit, self.delay, inner)
    }
}

#[derive(Debug, Clone)]
pub struct NailRater<S> {
    peers: Arc<HashMap<IpAddr, Peer, RandomWyHashState>>,
    limit: u64,
    delay: u64,
    schedule_pruning: Option<Instant>,
    inner: S,
}

impl<S> NailRater<S> {
    pub fn new(limit: u64, delay: u64, inner: S) -> Self {
        Self {
            peers: Default::default(),
            limit,
            delay,
            schedule_pruning: None,
            inner,
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
    type Future = NailedResponseFuture<S::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let Some(proxied) = extract(
            req.headers(),
            req.extensions().get::<ConnectInfo<SocketAddr>>(),
        ) else {
            return NailedResponseFuture {
                state: NailedState::Error {
                    response: Some((StatusCode::FORBIDDEN, "What are you hiding?").into_response()),
                },
            };
        };

        let prune = match self.schedule_pruning {
            None => {
                self.schedule_pruning = Some(Instant::now());
                None
            }
            Some(since) => {
                if since.elapsed() >= crate::SOURCE_TIMEOUT {
                    self.schedule_pruning = None;
                    Some({
                        let peers = self.peers.clone();

                        async move {
                            peers
                                .retain_async(|_, v| v.last_seen.elapsed() < crate::SOURCE_TIMEOUT)
                                .await
                        }
                        .boxed()
                    })
                } else {
                    None
                }
            }
        };

        let peer = self
            .peers
            .entry(proxied)
            .and_modify(|p| {
                p.count += 1;
                p.last_seen = Instant::now();
                if p.count >= self.limit {
                    p.state = PeerState::Limited;
                }
            })
            .or_insert_with(|| Peer {
                count: 1,
                state: PeerState::Ready,
                last_seen: Instant::now(),
            });

        if peer.state == PeerState::Limited {
            let inner = self.inner.call(req);
            let delay = sleep(Duration::from_millis(self.delay));

            NailedResponseFuture {
                state: NailedState::Limited {
                    future: inner,
                    delay: Box::pin(delay),
                    prune,
                },
            }
        } else {
            NailedResponseFuture {
                state: NailedState::Pass {
                    future: self.inner.call(req),
                    prune,
                },
            }
        }
    }
}

pin_project! {
    pub struct NailedResponseFuture<T> {
        #[pin]
        state: NailedState<T>,
    }
}

pin_project! {
    #[project = NailedStateProj]
    enum NailedState<T> {
        Pass {
            #[pin]
            future: T,
            prune: Option<Boxed<()>>,
        },
        Limited {
            #[pin]
            future: T,
            delay: Pin<Box<Sleep>>,
            prune: Option<Boxed<()>>,
        },
        Error {
            response: Option<Response<Body>>
        }
    }
}

impl<E, F> Future for NailedResponseFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.project().state.project() {
            NailedStateProj::Pass { future, prune } => {
                if let Some(mut prune) = prune.take() {
                    ready!(prune.as_mut().poll(cx));
                }

                future.poll(cx)
            }
            NailedStateProj::Limited {
                future,
                delay,
                prune,
            } => {
                if let Some(mut prune) = prune.take() {
                    ready!(prune.as_mut().poll(cx));
                }

                ready!(delay.as_mut().poll(cx));

                future.poll(cx)
            }
            NailedStateProj::Error { response } => Poll::Ready(Ok(response.take().expect("HUH"))),
        }
    }
}

fn extract(headers: &HeaderMap, connection: Option<&ConnectInfo<SocketAddr>>) -> Option<IpAddr> {
    maybe_x_forwarded_for(headers)
        .or_else(|| maybe_x_real_ip(headers))
        .or_else(|| maybe_forwarded(headers))
        .or_else(|| connection.map(|connect_info| connect_info.ip()))
}

/// Tries to parse the `x-forwarded-for` header
fn maybe_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_FORWARDED_FOR)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|header| {
            header
                .split(',')
                .map(str::trim)
                .filter(|&header_parts| !header_parts.is_empty())
                .find_map(|part| part.parse().ok())
        })
}

/// Tries to parse the `x-real-ip` header
fn maybe_x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_REAL_IP)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|header| header.trim().parse().ok())
}

/// Tries to parse `forwarded` headers
fn maybe_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    headers.get_all(FORWARDED).iter().find_map(|header_value| {
        header_value.to_str().ok().and_then(|header| {
            header
                .split(&[',', ';'])
                .map(str::trim)
                .filter(|&header_parts| !header_parts.is_empty())
                .find_map(|header_parts| extract_for.parse(header_parts).ok())
        })
    })
}
