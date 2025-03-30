//! This is basically a bastardisation of `<https://github.com/abdolence/axum-streams-rs>`, but
//! with less finesse.
//!
//! The whole purpose of all this is to be able to stream raw (UTF-8) bytes as an HTTP body.
//! We can rely on UTF-8 since the bytes have once been a string in Rust. I think. Who cares,
//! I think the bot reading the thing won't have time to care...

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    http::{HeaderMap, Response},
    response::IntoResponse,
};
use hyper::body::{Bytes, Frame};
use futures_lite::{stream::Boxed, Stream, StreamExt};

pub struct StreamBody {
    stream: Boxed<Result<hyper::body::Frame<axum::body::Bytes>, axum::Error>>,
    trailers: Option<HeaderMap>,
}

impl StreamBody {
    pub fn from_stream(stream: impl Stream<Item = Bytes> + Send + 'static) -> Self {
        Self {
            stream: Box::pin(stream.map(Bytes::from).map(Frame::data).map(Ok)),
            trailers: None,
        }
    }
}

impl StreamBody {
    /// Set headers for the body.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.trailers = Some(headers);
        self
    }
}

impl hyper::body::Body for StreamBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

impl IntoResponse for StreamBody {
    fn into_response(mut self) -> Response<Body> {
        let headers = if let Some(trailers) = self.trailers.take() {
            trailers
        } else {
            HeaderMap::new()
        };

        let mut response: Response<Body> = Response::new(Body::new(self));
        *response.headers_mut() = headers;
        response
    }
}