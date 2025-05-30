#![forbid(unsafe_code)]
use std::sync::LazyLock;

use axum::http::HeaderValue;
use hyper::{HeaderMap, header::CONTENT_TYPE};

pub mod inputs;
pub mod otel;
pub mod routes;
pub mod shutdown;
pub mod state;

static INDEX: &str = include_str!("../templates/warning.html");

static GEN_HEADER: LazyLock<HeaderMap> = LazyLock::new(|| {
    let mut headers = HeaderMap::new();
    headers.append(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers
});
