use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};
use hyper::{HeaderMap, header::FORWARDED};
use winnow::Parser;

use crate::fv_parser::{Identifier, extract_for};

const X_REAL_IP: &str = "x-real-ip";
const X_FORWARDED_FOR: &str = "x-forwarded-for";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(align(8))]
pub struct ProxiedPeer(IpAddr);

impl ProxiedPeer {
    pub fn extract(headers: &HeaderMap, connection: &ConnectInfo<SocketAddr>) -> Self {
        Self(
            maybe_x_forwarded_for(headers)
                .or_else(|| maybe_x_real_ip(headers))
                .or_else(|| maybe_forwarded(headers))
                .unwrap_or_else(|| connection.ip()),
        )
    }

    pub fn ip(&self) -> IpAddr {
        self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ProxiedPeer {
    type Rejection = Infallible;

    async fn from_request_parts(req: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::extract(
            &req.headers,
            req.extensions.get::<ConnectInfo<SocketAddr>>().unwrap(),
        ))
    }
}

/// Tries to parse the `x-forwarded-for` header
pub fn maybe_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_FORWARDED_FOR)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| s.split(',').find_map(|s| s.trim().parse::<IpAddr>().ok()))
}

/// Tries to parse the `x-real-ip` header
pub fn maybe_x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_REAL_IP)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| s.parse::<IpAddr>().ok())
}

/// Tries to parse `forwarded` headers
pub fn maybe_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    headers.get_all(FORWARDED).iter().find_map(|hv| {
        hv.to_str()
            .ok()
            .and_then(|s| {
                for sl in s.trim().split([',', ';']).filter(|&a| !a.is_empty()) {
                    if let Ok(ip) = extract_for.parse(sl) {
                        return Some(ip);
                    }
                }

                None
            })
            .map(|f| match f {
                Identifier::SocketAddr(socket_addr) => socket_addr.ip(),
                Identifier::IpAddr(ip_addr) => ip_addr,
            })
    })
}
