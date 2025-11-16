use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    body::{Body, Bytes},
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use futures_lite::{future::Boxed, ready};
use hyper::{
    StatusCode,
    header::{CONTENT_ENCODING, CONTENT_TYPE},
};
use nailspicy::SpicyPayloadKind;
use pin_project_lite::pin_project;
use tokio::time::Sleep;

pin_project! {
    pub struct NailedResponseFuture<T> {
        #[pin]
        state: NailedState<T>,
    }
}

pin_project! {
    #[project = NailedStateProj]
    enum NailedState<T> {
        Normal {
            #[pin]
            state: NailedNormalFuture<T>,
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

enum NailedOtherState {
    Dropped,
    Spicy {
        payload: Bytes,
        kind: SpicyPayloadKind,
    },
    Error,
    Finished,
}

impl<T> NailedResponseFuture<T> {
    #[inline]
    pub fn normal(prune: Option<Boxed<()>>, delay: Option<Pin<Box<Sleep>>>, inner: T) -> Self {
        let fut = match (prune, delay) {
            (Some(prune), delay) => NailedState::Normal {
                state: NailedNormalFuture {
                    state: NormalState::Prune { prune, delay },
                    inner,
                },
            },
            (None, Some(delay)) => NailedState::Normal {
                state: NailedNormalFuture {
                    state: NormalState::Delay { delay },
                    inner,
                },
            },
            (None, None) => NailedState::Normal {
                state: NailedNormalFuture {
                    state: NormalState::Pass,
                    inner,
                },
            },
        };

        Self { state: fut }
    }

    #[inline]
    pub fn dropped() -> Self {
        Self {
            state: NailedState::Other {
                state: NailedOtherState::Dropped,
            },
        }
    }

    #[inline]
    pub fn spicy(payload: Bytes, kind: SpicyPayloadKind) -> Self {
        Self {
            state: NailedState::Other {
                state: NailedOtherState::Spicy { payload, kind },
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

impl<E, F> Future for NailedResponseFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().state.project() {
            NailedStateProj::Normal { state } => state.poll(cx),
            NailedStateProj::Other { state } => Poll::Ready(Ok(state.build_response())),
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
