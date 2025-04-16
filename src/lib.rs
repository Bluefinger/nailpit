#![warn(clippy::undocumented_unsafe_blocks)]

use std::{sync::LazyLock, time::Duration};

use axum::http::HeaderValue;
use hyper::{header::CONTENT_TYPE, HeaderMap};

pub mod inputs;
mod peer;
pub mod routes;
pub mod shutdown;
pub mod state;

static INDEX: &str = include_str!("../templates/warning.html");

pub const SOURCE_TIMEOUT: Duration = Duration::from_secs(60 * 2);

static GEN_HEADER: LazyLock<HeaderMap> = LazyLock::new(|| {
    let mut headers = HeaderMap::new();
    headers.append(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers
});

