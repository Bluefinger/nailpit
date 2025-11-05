use crate::{
    root_span::RootSpan,
    root_span_builder::{self, RootSpanBuilder},
};

use actix_web::{
    Error, HttpMessage,
    body::{BodySize, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    web::Bytes,
};
#[cfg(feature = "emit_event_on_error")]
use actix_web::{ResponseError, http::StatusCode};
use core::future::{Future, Ready, ready};
use core::pin::Pin;
use core::task::{Context, Poll};
use nailip::IdentifiedPeer;
use nailrequest::RequestId;
use tracing::Span;

#[derive(Debug, Default, Clone, Copy)]
pub enum BuilderKind {
    #[default]
    Default,
    Minimal,
}

/// `TracingLogger` is a middleware to capture structured diagnostic when processing an HTTP request.
/// Check the crate-level documentation for an in-depth introduction.
///
/// `TracingLogger` is designed as a drop-in replacement of [`actix-web`]'s [`Logger`].
///
/// Like [`actix-web`]'s [`Logger`], in order to use `TracingLogger` inside a Scope, Resource, or
/// Condition, the [`Compat`] middleware must be used.
///
/// ```rust
/// use actix_web::middleware::Compat;
/// use actix_web::{web, App};
/// use nailtrace::middleware::TracingLogger;
///
/// let app = App::new()
///     .service(
///         web::scope("/some/route")
///             .wrap(Compat::new(TracingLogger::default())),
///     );
/// ```
///
/// [`actix-web`]: https://docs.rs/actix-web
/// [`Logger`]: https://docs.rs/actix-web/4.0.0-beta.13/actix_web/middleware/struct.Logger.html
/// [`Compat`]: https://docs.rs/actix-web/4.0.0-beta.13/actix_web/middleware/struct.Compat.html
/// [`tracing`]: https://docs.rs/tracing
#[derive(Debug, Clone, Default)]
pub struct TracingLogger {
    root_span_builder: BuilderKind,
}

impl TracingLogger {
    pub fn new(kind: BuilderKind) -> TracingLogger {
        TracingLogger {
            root_span_builder: kind,
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for TracingLogger
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<StreamSpan<B>>;
    type Error = Error;
    type Transform = TracingLoggerMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TracingLoggerMiddleware {
            service,
            root_span_builder: self.root_span_builder,
        }))
    }
}

#[doc(hidden)]
pub struct TracingLoggerMiddleware<S> {
    service: S,
    root_span_builder: BuilderKind,
}

impl<S, B> Service<ServiceRequest> for TracingLoggerMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<StreamSpan<B>>;
    type Error = Error;
    type Future = TracingResponse<S::Future>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let peer = IdentifiedPeer::extract(&req.connection_info());

        {
            let mut extensions = req.extensions_mut();
            extensions.insert(peer);
            extensions.insert(RequestId::generate());
        }

        let root_span = match &self.root_span_builder {
            BuilderKind::Default => {
                root_span_builder::DefaultRootSpanBuilder::on_request_start(&req)
            }
            BuilderKind::Minimal => {
                root_span_builder::MinimalRootSpanBuilder::on_request_start(&req)
            }
        };

        let root_span_wrapper = RootSpan::new(root_span.clone());
        req.extensions_mut().insert(root_span_wrapper);

        let fut = root_span.in_scope(|| self.service.call(req));

        TracingResponse {
            fut,
            span: root_span,
            root_span_type: self.root_span_builder,
        }
    }
}

pin_project_lite::pin_project! {
    #[doc(hidden)]
    pub struct TracingResponse<F> {
        #[pin]
        fut: F,
        span: Span,
        root_span_type: BuilderKind,
    }
}

pin_project_lite::pin_project! {
    #[doc(hidden)]
    pub struct StreamSpan<B> {
        #[pin]
        body: B,
        span: Span,
    }
}

impl<F, B> Future for TracingResponse<F>
where
    F: Future<Output = Result<ServiceResponse<B>, Error>>,
    B: MessageBody + 'static,
{
    type Output = Result<ServiceResponse<StreamSpan<B>>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let fut = this.fut;
        let span = this.span;

        span.in_scope(|| match fut.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(outcome) => {
                match &this.root_span_type {
                    BuilderKind::Default => {
                        root_span_builder::DefaultRootSpanBuilder::on_request_end(
                            Span::current(),
                            &outcome,
                        )
                    }
                    BuilderKind::Minimal => {
                        root_span_builder::MinimalRootSpanBuilder::on_request_end(
                            Span::current(),
                            &outcome,
                        )
                    }
                };

                #[cfg(feature = "emit_event_on_error")]
                {
                    emit_event_on_error(&outcome);
                }

                Poll::Ready(outcome.map(|service_response| {
                    service_response.map_body(|_, body| StreamSpan {
                        body,
                        span: span.clone(),
                    })
                }))
            }
        })
    }
}

impl<B> MessageBody for StreamSpan<B>
where
    B: MessageBody,
{
    type Error = B::Error;

    fn size(&self) -> BodySize {
        self.body.size()
    }

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        let this = self.project();

        let body = this.body;
        let span = this.span;
        span.in_scope(|| body.poll_next(cx))
    }
}

#[cfg(feature = "emit_event_on_error")]
fn emit_event_on_error<B: 'static>(outcome: &Result<ServiceResponse<B>, actix_web::Error>) {
    match outcome {
        Ok(response) => {
            if let Some(err) = response.response().error() {
                // use the status code already constructed for the outgoing HTTP response
                emit_error_event(err.as_response_error(), response.status())
            }
        }
        Err(error) => {
            let response_error = error.as_response_error();
            emit_error_event(response_error, response_error.status_code())
        }
    }
}

#[cfg(feature = "emit_event_on_error")]
fn emit_error_event(response_error: &dyn ResponseError, status_code: StatusCode) {
    let error_msg_prefix = "Error encountered while processing the incoming HTTP request";
    if status_code.is_client_error() {
        tracing::warn!("{}: {:?}", error_msg_prefix, response_error);
    } else {
        tracing::error!("{}: {:?}", error_msg_prefix, response_error);
    }
}
