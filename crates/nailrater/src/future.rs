use std::{
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::{
    HttpResponse,
    dev::{ServiceRequest, ServiceResponse},
    http::header::{CONTENT_ENCODING, ContentType},
    mime,
    web::Bytes,
};
use futures_lite::{future::Boxed, ready};
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
        Other { state: NailedOtherState }
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
    Dropped {
        req: ServiceRequest,
    },
    Spicy {
        req: ServiceRequest,
        payload: Bytes,
        kind: SpicyPayloadKind,
    },
    Error {
        req: ServiceRequest,
    },
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
    pub fn dropped(req: ServiceRequest) -> Self {
        Self {
            state: NailedState::Other {
                state: NailedOtherState::Dropped { req },
            },
        }
    }

    #[inline]
    pub fn spicy(req: ServiceRequest, payload: Bytes, kind: SpicyPayloadKind) -> Self {
        Self {
            state: NailedState::Other {
                state: NailedOtherState::Spicy { req, payload, kind },
            },
        }
    }

    #[inline]
    pub fn error(req: ServiceRequest) -> Self {
        Self {
            state: NailedState::Other {
                state: NailedOtherState::Error { req },
            },
        }
    }
}

impl<E, F> Future for NailedResponseFuture<F>
where
    F: Future<Output = Result<ServiceResponse, E>>,
{
    type Output = Result<ServiceResponse, E>;

    #[cfg_attr(
        feature = "detailed_traces",
        tracing::instrument(name = "Response Future", level = "trace", skip_all)
    )]
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
    F: Future<Output = Result<ServiceResponse, E>>,
{
    type Output = Result<ServiceResponse, E>;

    #[cfg_attr(
        feature = "detailed_traces",
        tracing::instrument(name = "Normal Response", level = "trace", skip_all)
    )]
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
    fn build_response(&mut self) -> ServiceResponse {
        let state = core::mem::replace(self, Self::Finished);

        match state {
            Self::Dropped { req } => {
                req.into_response(HttpResponse::TooManyRequests().body("Go away."))
            }
            Self::Spicy { req, payload, kind } => {
                let response = HttpResponse::TooManyRequests()
                    .insert_header(ContentType(mime::TEXT_HTML_UTF_8))
                    .insert_header((CONTENT_ENCODING, kind.as_str()))
                    .body(payload);

                req.into_response(response)
            }
            Self::Error { req } => req.into_response(
                HttpResponse::InternalServerError().body("Something went wrong here."),
            ),
            Self::Finished => unreachable!("Response future has been polled more than once"),
        }
    }
}
