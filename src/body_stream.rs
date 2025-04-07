use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    http::{HeaderMap, Response},
    response::IntoResponse,
};
use futures_lite::{Stream, StreamExt, stream::Boxed};
use hyper::body::{Bytes, Frame};

pub struct BodyStream {
    stream: Boxed<Result<Frame<Bytes>, axum::Error>>,
    headers: Option<HeaderMap>,
}

impl BodyStream {
    pub fn from_stream(stream: impl Stream<Item = Bytes> + Send + 'static) -> Self {
        Self {
            stream: stream.map(Frame::data).map(Ok).boxed(),
            headers: None,
        }
    }
}

impl BodyStream {
    /// Set headers for the body.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers = Some(headers);
        self
    }
}

impl hyper::body::Body for BodyStream {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        self.stream.poll_next(cx)
    }
}

impl IntoResponse for BodyStream {
    fn into_response(mut self) -> Response<Body> {
        let headers = self.headers.take().unwrap_or_default();
        let mut response: Response<Body> = Response::new(Body::new(self));
        *response.headers_mut() = headers;
        response
    }
}
