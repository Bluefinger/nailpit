use std::borrow::Cow;

use axum::{
    Extension,
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use hyper::{
    HeaderMap, Uri, Version,
    body::Body,
    header::{CONTENT_ENCODING, CONTENT_TYPE, HOST, USER_AGENT},
};
use nailip::{IdentifiedPeer, header_value_to_str};
use opentelemetry::Context;
use opentelemetry_http::HeaderExtractor;
use opentelemetry_semantic_conventions::{
    attribute::OTEL_STATUS_CODE, trace::HTTP_RESPONSE_STATUS_CODE,
};
use tower_http::request_id::RequestId;
use tracing::{Span, field::Empty, info_span};

pub fn extract_context(headers: &HeaderMap) -> Context {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(headers))
    })
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

pub async fn trace_connection_layer(
    request_id: Extension<RequestId>,
    peer: Extension<IdentifiedPeer>,
    req: Request,
    next: Next,
) -> Response<StreamSpan<axum::body::Body>> {
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let headers = req.headers();

    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map_or("not-matched", MatchedPath::as_str);

    let http_method = req.method().as_str();

    let host = headers.get(HOST).and_then(header_value_to_str);

    let root_name = format!("{http_method} {path}");

    let span = info_span!(
        "HTTP request",
        http.method = %http_method,
        http.route = path, // to set by router of "webframework" after
        http.host = Empty,
        http.flavor = %http_flavor(req.version()),
        http.client.address = peer.to_string(), //%$request.connection_info().realip_remote_addr().unwrap_or(""),
        user_agent.original = headers
            .get(USER_AGENT)
            .and_then(header_value_to_str)
            .unwrap_or("None"),
        http.response.status_code = Empty, // to set on response
        http.response.header.content_encoding = Empty,
        http.response.header.content_type = Empty,
        url.path = req.uri().path(),
        url.query = req.uri().query(),
        http.scheme = url_scheme(req.uri()),
        otel.name = root_name, // to set by router of "webframework" after
        otel.kind = "server",
        otel.status_code = Empty, // to set on response
        trace_id = Empty, // to set on response
        http.request.header.request_id = Empty, // to set
        exception.message = Empty, // to set on response
    );

    if let Some(request_id) = header_value_to_str(request_id.header_value()) {
        span.record("http.request.header.request_id", request_id);
    }

    if let Some(host) = host {
        span.record("http.host", host);
    }

    let _ = span.set_parent(extract_context(headers));

    let response = InspectHttpResponse {
        inner: next.run(req),
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

pin_project_lite::pin_project! {
    #[doc(hidden)]
    pub struct StreamSpan<B> {
        #[pin]
        body: B,
        span: Span,
    }
}

impl<F> core::future::Future for InspectHttpResponse<F>
where
    F: core::future::Future<Output = Response>,
{
    type Output = Response<StreamSpan<axum::body::Body>>;

    #[inline]
    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.project();

        let span = this.span;

        let poll = span.in_scope(|| this.inner.poll(cx));

        if let core::task::Poll::Ready(response) = poll {
            let status = response.status();
            let headers = response.headers();

            span.record(HTTP_RESPONSE_STATUS_CODE, status.as_u16());

            if let Some(encoding) = headers.get(CONTENT_ENCODING).and_then(header_value_to_str) {
                span.record("http.response.header.content_encoding", encoding);
            }

            if let Some(content_type) = headers.get(CONTENT_TYPE).and_then(header_value_to_str) {
                span.record("http.response.header.content_type", content_type);
            }

            if status.is_server_error() {
                span.record(OTEL_STATUS_CODE, "ERROR");
            } else {
                span.record(OTEL_STATUS_CODE, "OK");
            }

            core::task::Poll::Ready(response.map(|body| StreamSpan {
                body,
                span: span.clone(),
            }))
        } else {
            core::task::Poll::Pending
        }
    }
}

impl<B> Body for StreamSpan<B>
where
    B: Body,
{
    type Data = B::Data;
    type Error = B::Error;

    #[inline]
    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let this = self.project();

        let body = this.body;
        let span = this.span;
        span.in_scope(|| body.poll_frame(cx))
    }
}
