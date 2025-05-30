use std::task::Context;
use std::task::Poll;

use fastrace::prelude::*;
use hyper::Request;
use hyper::header::USER_AGENT;
use tower::{Layer, Service};
use tower_http::request_id::RequestId;

/// The standard [W3C Trace Context](https://www.w3.org/TR/trace-context/) header name for passing trace information.
///
/// This is the header key used to propagate trace context between services according to
/// the W3C Trace Context specification.
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Server layer for intercepting and processing trace context in incoming requests.
///
/// This layer extracts tracing context from incoming requests and creates a new span
/// for each request. Add this to your tower server to automatically handle trace context
/// propagation.
#[derive(Clone)]
pub struct NailTraceLayer;

impl<S> Layer<S> for NailTraceLayer {
    type Service = NailTraceService<S>;

    fn layer(&self, service: S) -> Self::Service {
        NailTraceService { service }
    }
}

/// Server-side service that handles trace context propagation.
///
/// This service extracts trace context from incoming requests and creates
/// spans to track the request processing. It wraps the inner service and augments
/// it with tracing capabilities. Appends the `x-request-id` to the root span if present,
/// else it generates an id for tracking individual requests.
#[derive(Clone)]
pub struct NailTraceService<S> {
    service: S,
}

impl<S, Body> Service<Request<Body>> for NailTraceService<S>
where
    S: Service<Request<Body>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = fastrace::future::InSpan<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let headers = req.headers();
        let extensions = req.extensions();

        let parent = headers
            .get(TRACEPARENT_HEADER)
            .and_then(|traceparent| SpanContext::decode_w3c_traceparent(traceparent.to_str().ok()?))
            .unwrap_or_else(SpanContext::random);

        let root = Span::root(req.uri().to_string(), parent).with_properties(|| {
            [
                (
                    "request.id",
                    extensions
                        .get::<RequestId>()
                        .and_then(|header| header.header_value().to_str().ok())
                        .map_or_else(|| uuid::Uuid::new_v4().to_string(), ToString::to_string),
                ),
                (
                    "user.agent",
                    headers
                        .get(USER_AGENT)
                        .and_then(|header| header.to_str().ok())
                        .map_or_else(|| "None".to_string(), ToString::to_string),
                ),
            ]
        });

        self.service.call(req).in_span(root)
    }
}
