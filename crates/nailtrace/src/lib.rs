use axum::{Extension, extract::Request, middleware::Next, response::Response};
use fastrace::prelude::*;
use hyper::header::USER_AGENT;
use nailip::IdentifiedPeer;
use tower_http::request_id::RequestId;

/// The standard [W3C Trace Context](https://www.w3.org/TR/trace-context/) header name for passing trace information.
///
/// This is the header key used to propagate trace context between services according to
/// the W3C Trace Context specification.
pub const TRACEPARENT_HEADER: &str = "traceparent";

pub async fn tracing_root_span(
    request_id: Extension<RequestId>,
    peer: Extension<IdentifiedPeer>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let parent = headers
        .get(TRACEPARENT_HEADER)
        .and_then(|traceparent| SpanContext::decode_w3c_traceparent(traceparent.to_str().ok()?))
        .unwrap_or_else(SpanContext::random);

    let root_name = if request.uri() == "/" {
        String::from("Index Page")
    } else {
        String::from("Generated Page")
    };

    let root = Span::root(root_name, parent).with_properties(|| {
        [
            (
                "request.id",
                request_id
                    .header_value()
                    .to_str()
                    .map_or_else(|_| uuid::Uuid::new_v4().to_string(), ToString::to_string),
            ),
            ("request.uri", request.uri().to_string()),
            (
                "user.agent",
                headers
                    .get(USER_AGENT)
                    .and_then(|header| header.to_str().ok())
                    .map_or_else(|| "None".to_string(), ToString::to_string),
            ),
            ("user.ip", peer.to_string()),
        ]
    });

    next.run(request).in_span(root).await
}
