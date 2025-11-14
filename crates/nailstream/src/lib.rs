//! Util for defining a customised HTTP stream response, but for text/html or other headers.

use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    http::{HeaderValue, Response},
    response::IntoResponse,
};
use futures_lite::Stream;
use hyper::{
    body::{Bytes, Frame},
    header::CONTENT_TYPE,
};

const CONTENT_TYPE_VALUE: HeaderValue = HeaderValue::from_static("text/html; charset=utf-8");

pub struct NailResponseStream<S> {
    stream: S,
}

impl<S> NailResponseStream<S>
where
    S: Stream<Item = Bytes> + Unpin + Send + 'static,
{
    #[inline]
    pub fn from_stream(stream: S) -> Self {
        Self { stream }
    }
}

impl<S> hyper::body::Body for NailResponseStream<S>
where
    S: Stream<Item = Bytes> + Unpin + Send + 'static,
{
    type Data = Bytes;
    type Error = Infallible;

    #[inline(always)]
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match core::pin::pin!(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(bytes)) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S> IntoResponse for NailResponseStream<S>
where
    S: Stream<Item = Bytes> + Unpin + Send + 'static,
{
    #[inline]
    fn into_response(self) -> Response<Body> {
        let mut response: Response<Body> = Response::new(Body::new(self));
        response
            .headers_mut()
            .append(CONTENT_TYPE, CONTENT_TYPE_VALUE);
        response
    }
}
