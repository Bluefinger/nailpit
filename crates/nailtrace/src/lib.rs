use std::borrow::Cow;

use axum::{
    body::{Body as AxumBody, Bytes},
    Extension,
    extract::{MatchedPath, Request},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use hyper::{
    HeaderMap, Uri, Version,
    body::Body,
    header::{CONTENT_ENCODING, CONTENT_TYPE, USER_AGENT},
};
use nailip::{IdentifiedPeer, header_value_to_str};
use opentelemetry::Context;
use opentelemetry_http::HeaderExtractor;
use opentelemetry_semantic_conventions::{
    attribute::OTEL_STATUS_CODE,
    trace::{ERROR_TYPE, HTTP_RESPONSE_STATUS_CODE},
};
use tracing::{Span, field::Empty, info_span};
use uuid::Uuid;

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
    peer: Extension<IdentifiedPeer>,
    req: Request,
    next: Next,
) -> Response<InspectBody<AxumBody>> {
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let headers = req.headers();

    let request_id = headers.get("x-request-id").cloned().unwrap_or_else(
        // SAFETY: The UUID is converted to a valid UTF-8 string before being turned into
        // Bytes. As such, the Bytes instance corresponds to a valid internal repr for
        // HeaderValue, meaning we can skip validation directly.
        || unsafe {
            HeaderValue::from_maybe_shared_unchecked(Bytes::from(
                Uuid::now_v7().to_string(),
            ))
        },
    );

    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map_or("not-matched", MatchedPath::as_str);

    let http_method = req.method().as_str();

    let root_name = format!("{http_method} {path}");

    let span = info_span!(
        "HTTP request",
        http.request.method = %http_method,
        http.route = path, // to set by router of "webframework" after
        network.protocol.version = %http_flavor(req.version()),
        client.address = peer.ip().to_string(), //%$request.connection_info().realip_remote_addr().unwrap_or(""),
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
        error.type = Empty,
    );

    if let Some(request_id) = header_value_to_str(&request_id) {
        span.record("http.request.header.request_id", request_id);
    }

    let _ = span.set_parent(extract_context(headers));

    let inner = span.in_scope(|| next.run(req));

    let response = InspectHttpResponse {
        inner,
        span: InspectState::Ready { span, request_id },
    };

    response.await
}

enum InspectState {
    Ready { span: Span, request_id: HeaderValue },
    Finished,
}

impl InspectState {
    #[inline]
    fn span_ref(&self) -> &Span {
        match self {
            Self::Ready { span, .. } => span,
            Self::Finished => unreachable!("Invalid state, future was polled after completion"),
        }
    }

    #[inline]
    fn take(&mut self) -> (Span, HeaderValue) {
        let span = core::mem::replace(self, Self::Finished);

        match span {
            Self::Ready { span, request_id } => (span, request_id),
            Self::Finished => unreachable!("Invalid state, future was polled after completion"),
        }
    }
}

pin_project_lite::pin_project! {
    struct InspectHttpResponse<F> {
        #[pin]
        inner: F,
        span: InspectState,
    }
}

pin_project_lite::pin_project! {
    #[doc(hidden)]
    pub struct InspectBody<B> {
        #[pin]
        body: B,
        span: Span,
    }
}

impl<F> core::future::Future for InspectHttpResponse<F>
where
    F: core::future::Future<Output = Response>,
{
    type Output = Response<InspectBody<AxumBody>>;

    #[inline]
    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.project();

        let span = this.span.span_ref();

        let poll = span.in_scope(|| this.inner.poll(cx));

        if let core::task::Poll::Ready(mut response) = poll {
            let status = response.status();
            let headers = response.headers();

            span.record(HTTP_RESPONSE_STATUS_CODE, status.as_u16());

            if let Some(encoding) = headers.get(CONTENT_ENCODING).and_then(header_value_to_str) {
                span.record("http.response.header.content_encoding", encoding);
            }

            if let Some(content_type) = headers.get(CONTENT_TYPE).and_then(header_value_to_str) {
                span.record("http.response.header.content_type", content_type);
            }

            if status.is_client_error() || status.is_server_error() {
                span.record(ERROR_TYPE, status.as_u16());
            }

            if status.is_server_error() {
                span.record(OTEL_STATUS_CODE, "ERROR");
            } else {
                span.record(OTEL_STATUS_CODE, "OK");
            }

            let (span, request_id) = this.span.take();

            response.headers_mut().insert("x-request-id", request_id);

            core::task::Poll::Ready(response.map(|body| InspectBody { body, span }))
        } else {
            core::task::Poll::Pending
        }
    }
}

impl<B> Body for InspectBody<B>
where
    B: Body,
{
    type Data = B::Data;
    type Error = B::Error;

    #[inline(always)]
    fn poll_frame(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let this = self.project();

        this.span.in_scope(|| this.body.poll_frame(cx))
    }
}
