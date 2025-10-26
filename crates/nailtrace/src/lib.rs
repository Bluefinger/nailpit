use core::net::SocketAddr;
use std::borrow::Cow;

use axum::{
    Extension,
    extract::{ConnectInfo, MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use hyper::{
    Uri, Version,
    header::{CONTENT_ENCODING, CONTENT_TYPE, HOST, USER_AGENT},
};
use nailip::{IdentifiedPeer, header_value_to_str};
use opentelemetry_semantic_conventions::{
    attribute::OTEL_STATUS_CODE,
    trace::{HTTP_RESPONSE_STATUS_CODE, SERVER_ADDRESS, SERVER_PORT},
};
use tower_http::request_id::RequestId;
use tracing::{Span, field::Empty};
use tracing_opentelemetry_instrumentation_sdk::{
    http::{self as otel_http},
    otel_trace_span,
};

pub fn decode_w3c_traceparent(traceparent: &str) -> Option<(u128, u64, bool)> {
    let mut parts = traceparent.split('-');

    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("00"), Some(trace_id), Some(span_id), Some(sampled), None) => {
            let trace_id = u128::from_str_radix(trace_id, 16).ok()?;
            let span_id = u64::from_str_radix(span_id, 16).ok()?;
            let sampled = u8::from_str_radix(sampled, 16).ok()? & 1 == 1;
            if trace_id == 0 || span_id == 0 {
                return None;
            }
            Some((trace_id, span_id, sampled))
        }
        _ => None,
    }
}

#[inline]
pub fn url_scheme(uri: &Uri) -> &str {
    uri.scheme_str().unwrap_or_default()
}

#[inline]
#[must_use]
pub fn http_flavor(version: Version) -> Cow<'static, str> {
    match version {
        Version::HTTP_09 => "0.9".into(),
        Version::HTTP_10 => "1.0".into(),
        Version::HTTP_11 => "1.1".into(),
        Version::HTTP_2 => "2.0".into(),
        Version::HTTP_3 => "3.0".into(),
        other => format!("{other:?}").into(),
    }
}

/// The standard [W3C Trace Context](https://www.w3.org/TR/trace-context/) header name
/// for passing trace information.
///
/// This is the header key used to propagate trace context between services according to
/// the W3C Trace Context specification.
pub const TRACEPARENT_HEADER: &str = "traceparent";

pub async fn tracing_root_span(
    request_id: Extension<RequestId>,
    peer: Extension<IdentifiedPeer>,
    connection: ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    let headers = req.headers();

    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map_or("not-matched", MatchedPath::as_str);

    let http_method = req.method().as_str();

    let mut host = headers
        .get(HOST)
        .and_then(header_value_to_str)
        .into_iter()
        .flat_map(|header| header.trim().split(":"));

    let root_name = format!("{http_method} {path}");

    let span = otel_trace_span!(
        "HTTP request",
        http.request.method = %http_method,
        http.route = path, // to set by router of "webframework" after
        network.protocol.version = %http_flavor(req.version()),
        server.address = Empty,
        server.port = Empty,
        http.client.address = peer.to_string(), //%$request.connection_info().realip_remote_addr().unwrap_or(""),
        user_agent.original = headers
            .get(USER_AGENT)
            .and_then(header_value_to_str)
            .unwrap_or("None"),
        http.response.status_code = Empty, // to set on response
        url.path = req.uri().path(),
        url.query = req.uri().query(),
        url.scheme = url_scheme(req.uri()),
        otel.name = root_name, // to set by router of "webframework" after
        otel.kind = "server",
        otel.status_code = Empty, // to set on response
        trace_id = Empty, // to set on response
        request_id = Empty, // to set
        exception.message = Empty, // to set on response
    );

    if let Some(request_id) = header_value_to_str(request_id.header_value()) {
        span.record("http.request.header.request_id", request_id);
    }

    if let Some(host) = host.next() {
        span.record(SERVER_ADDRESS, host);
    }

    if let Some(port) = host.next() {
        span.record(SERVER_PORT, port);
    } else {
        span.record(SERVER_PORT, connection.ip().to_string());
    }

    let _ = span.set_parent(otel_http::extract_context(req.headers()));

    let response = InspectHttpResponse {
        inner: {
            let _guard = span.enter();
            next.run(req)
        },
        span,
    };

    response.await
}

pin_project_lite::pin_project! {
    struct InspectHttpResponse<F> {
        #[pin]
        inner: F,
        span: Span,
    }
}

impl<F> core::future::Future for InspectHttpResponse<F>
where
    F: core::future::Future<Output = Response>,
{
    type Output = F::Output;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.project();
        let _guard = this.span.enter();
        let poll = this.inner.poll(cx);

        if let core::task::Poll::Ready(response) = &poll {
            let status = response.status();
            let headers = response.headers();

            this.span.record(HTTP_RESPONSE_STATUS_CODE, status.as_u16());

            if let Some(encoding) = headers.get(CONTENT_ENCODING).and_then(header_value_to_str) {
                this.span
                    .record("http.response.header.content_encoding", encoding);
            }

            if let Some(content_type) = headers.get(CONTENT_TYPE).and_then(header_value_to_str) {
                this.span
                    .record("http.response.header.content_type", content_type);
            }

            if status.is_server_error() {
                this.span.record(OTEL_STATUS_CODE, "ERROR");
            }
        }

        poll
    }
}
