use axum::{
    Extension,
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use fastrace::prelude::*;
use hyper::header::{CONTENT_ENCODING, CONTENT_TYPE, USER_AGENT};
use nailip::IdentifiedPeer;
use opentelemetry_semantic_conventions::trace::{
    CLIENT_ADDRESS, HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE, URL_PATH,
    USER_AGENT_ORIGINAL,
};
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

    let path = if let Some(nested) = request.extensions().get::<MatchedPath>() {
        nested.as_str().to_string()
    } else {
        String::from("/")
    };

    let url_path = request.uri().path().to_string();

    let root_name = format!("GET {path}");

    let root = Span::root(root_name, parent);

    root.add_properties(|| {
        [
            (HTTP_ROUTE, path),
            (HTTP_REQUEST_METHOD, request.method().to_string()),
            (URL_PATH, url_path),
            (
                "http.request.header.request_id",
                request_id
                    .header_value()
                    .to_str()
                    .map_or_else(|_| uuid::Uuid::new_v4().to_string(), ToString::to_string),
            ),
            (
                USER_AGENT_ORIGINAL,
                headers
                    .get(USER_AGENT)
                    .and_then(|header| header.to_str().ok())
                    .map_or_else(|| "None".to_string(), ToString::to_string),
            ),
            (CLIENT_ADDRESS, peer.to_string()),
        ]
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

            if let Some((content_encoding, content_type)) = headers
                .get(CONTENT_ENCODING)
                .and_then(|header| header.to_str().ok())
                .map(ToString::to_string)
                .zip(
                    headers
                        .get(CONTENT_TYPE)
                        .and_then(|header| header.to_str().ok())
                        .map(ToString::to_string),
                )
            {
                LocalSpan::add_properties(|| {
                    [
                        ("http.response.header.content_encoding", content_encoding),
                        ("http.response.header.content_type", content_type),
                    ]
                });
            }

            LocalSpan::add_properties(|| {
                [(
                    HTTP_RESPONSE_STATUS_CODE,
                    response.status().as_u16().to_string(),
                )]
            });
        }

        poll
    }
}
