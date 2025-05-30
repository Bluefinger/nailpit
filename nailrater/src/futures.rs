use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use futures_lite::{future::Boxed, ready};
use hyper::StatusCode;
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
        Dropped,
        Error,
    }
}

impl<T> NailedResponseFuture<T> {
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

    pub fn dropped() -> Self {
        Self {
            state: NailedState::Dropped,
        }
    }

    pub fn error() -> Self {
        Self {
            state: NailedState::Error,
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

impl<E, F> Future for NailedResponseFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().state.project() {
            NailedStateProj::Normal { state } => state.poll(cx),
            NailedStateProj::Dropped => Poll::Ready(Ok(
                (StatusCode::TOO_MANY_REQUESTS, "Go away").into_response()
            )),
            NailedStateProj::Error => {
                Poll::Ready(Ok(
                    (StatusCode::FORBIDDEN, "What are you hiding?").into_response()
                ))
            }
        }
    }
}

impl<E, F> Future for NailedNormalFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

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
