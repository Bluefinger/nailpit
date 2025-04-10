use std::net::{IpAddr, SocketAddr};

use axum::extract::Request;
use hyper::{HeaderMap, header::FORWARDED};
use winnow::Parser;

use crate::fv_parser::{Identifier, extract_for};

const X_REAL_IP: &str = "x-real-ip";
const X_FORWARDED_FOR: &str = "x-forwarded-for";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerExtractor;

impl PeerExtractor {
    pub fn extract<T>(&self, req: &Request<T>) -> Option<IpAddr> {
        let headers = req.headers();

        maybe_x_forwarded_for(headers)
            .or_else(|| maybe_x_real_ip(headers))
            .or_else(|| maybe_forwarded(headers))
            .or_else(|| maybe_connect_info(req))
    }
}

/// Tries to parse the `x-forwarded-for` header
fn maybe_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_FORWARDED_FOR)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| s.split(',').find_map(|s| s.trim().parse::<IpAddr>().ok()))
}

/// Tries to parse the `x-real-ip` header
fn maybe_x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_REAL_IP)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| s.parse::<IpAddr>().ok())
}

/// Tries to parse `forwarded` headers
fn maybe_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
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

/// Looks in `ConnectInfo` extension
fn maybe_connect_info<T>(req: &Request<T>) -> Option<IpAddr> {
    req.extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|addr| addr.ip())
}
