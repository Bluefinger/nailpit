use std::iter::once;

use axum::{
    Extension,
    extract::{ConnectInfo, MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use fastrace::prelude::*;
use hyper::header::{CONTENT_ENCODING, CONTENT_TYPE, HOST, USER_AGENT};
use nailip::{IdentifiedPeer, header_value_to_str};
use nailnet::NailConnectionInfo;
use opentelemetry_semantic_conventions::trace::{
    CLIENT_ADDRESS, HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE, SERVER_ADDRESS,
    SERVER_PORT, URL_PATH, USER_AGENT_ORIGINAL,
};
use tower_http::request_id::RequestId;

/// The standard [W3C Trace Context](https://www.w3.org/TR/trace-context/) header name
/// for passing trace information.
///
/// This is the header key used to propagate trace context between services according to
/// the W3C Trace Context specification.
pub const TRACEPARENT_HEADER: &str = "traceparent";

pub async fn tracing_root_span(
    request_id: Extension<RequestId>,
    peer: Extension<IdentifiedPeer>,
    connection: ConnectInfo<NailConnectionInfo>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let parent = headers
        .get(TRACEPARENT_HEADER)
        .and_then(header_value_to_str)
        .and_then(SpanContext::decode_w3c_traceparent)
        .unwrap_or_else(SpanContext::random);

    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("not-matched", MatchedPath::as_str)
        .to_string();

    let method = request.method().to_string();

    let root_name = format!("{method} {path}");

    let root = Span::root(root_name, parent);

    root.add_properties(|| {
        let mut host = headers
            .get(HOST)
            .and_then(header_value_to_str)
            .into_iter()
            .flat_map(|header| header.trim().split(":"));

        once((HTTP_ROUTE, path))
            .chain(Some((HTTP_REQUEST_METHOD, method)))
            .chain(Some((URL_PATH, request.uri().path().to_string())))
            .chain(
                header_value_to_str(request_id.header_value())
                    .map(|request_id| ("http.request.header.request_id", request_id.to_string())),
            )
            .chain(Some((
                USER_AGENT_ORIGINAL,
                headers
                    .get(USER_AGENT)
                    .and_then(header_value_to_str)
                    .map_or_else(|| "None".to_string(), ToString::to_string),
            )))
            .chain(Some((CLIENT_ADDRESS, peer.to_string())))
            .chain(host.next().map(|host| (SERVER_ADDRESS, host.to_string())))
            .chain(Some((
                SERVER_PORT,
                host.next()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| connection.local.port().to_string()),
            )))
    });

    let response = InspectHttpResponse {
        inner: next.run(request),
    };

    response.in_span(root).await
}

pin_project_lite::pin_project! {
    struct InspectHttpResponse<F> {
        #[pin]
        inner: F,
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
        let poll = this.inner.poll(cx);

        if let core::task::Poll::Ready(response) = &poll {
            let headers = response.headers();

            LocalSpan::add_properties(|| {
                once((
                    HTTP_RESPONSE_STATUS_CODE,
                    response.status().as_u16().to_string(),
                ))
                .chain(
                    headers
                        .get(CONTENT_ENCODING)
                        .and_then(header_value_to_str)
                        .map(ToString::to_string)
                        .map(|content_encoding| {
                            ("http.response.header.content_encoding", content_encoding)
                        }),
                )
                .chain(
                    headers
                        .get(CONTENT_TYPE)
                        .and_then(header_value_to_str)
                        .map(ToString::to_string)
                        .map(|content_type| ("http.response.header.content_type", content_type)),
                )
            });
        }

        poll
    }
}
