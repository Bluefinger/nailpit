use std::net::{IpAddr, SocketAddr};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};
use hyper::{HeaderMap, StatusCode, header::FORWARDED};
use winnow::Parser;

use crate::fv_parser::extract_for;

const X_REAL_IP: &str = "x-real-ip";
const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// Extractor for obtaining an IP address from the request. Attempts to pull the IP from
/// various headers expected from a reverse proxied connection, else falls back to [`ConnectInfo`]
/// to get at least something if `nailpit` is not behind a proxy. If it can't get anything, then
/// something is seriously wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(align(8))]
pub struct ProxiedPeer(IpAddr);

impl ProxiedPeer {
    /// Extracts the IP address from either the request headers, or from the [`ConnectInfo`] extension
    /// if it can't. Returns `None` if it finds nothing.
    pub fn extract(
        headers: &HeaderMap,
        connection: Option<&ConnectInfo<SocketAddr>>,
    ) -> Option<Self> {
        maybe_x_forwarded_for(headers)
            .or_else(|| maybe_x_real_ip(headers))
            .or_else(|| maybe_forwarded(headers))
            .or_else(|| connection.map(|connect_info| connect_info.ip()))
            .map(Self)
    }

    /// Returns the extracted [`IpAddr`].
    #[inline]
    pub fn ip(&self) -> IpAddr {
        self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ProxiedPeer {
    type Rejection = (StatusCode, &'static str);

    #[fastrace::trace]
    async fn from_request_parts(req: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Self::extract(
            &req.headers,
            req.extensions.get::<ConnectInfo<SocketAddr>>(),
        )
        .ok_or((StatusCode::FORBIDDEN, "What are you hiding?"))
    }
}

/// Tries to parse the `x-forwarded-for` header
fn maybe_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_FORWARDED_FOR)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|header| {
            header
                .split(',')
                .map(str::trim)
                .filter(|&header_parts| !header_parts.is_empty())
                .find_map(|part| part.parse().ok())
        })
}

/// Tries to parse the `x-real-ip` header
fn maybe_x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_REAL_IP)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|header| header.trim().parse().ok())
}

/// Tries to parse `forwarded` headers
fn maybe_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    headers.get_all(FORWARDED).iter().find_map(|header_value| {
        header_value.to_str().ok().and_then(|header| {
            header
                .split(&[',', ';'])
                .map(str::trim)
                .filter(|&header_parts| !header_parts.is_empty())
                .find_map(|header_parts| extract_for.parse(header_parts).ok())
        })
    })
}
