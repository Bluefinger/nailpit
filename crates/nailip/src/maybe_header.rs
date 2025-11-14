use std::net::IpAddr;

use axum::http::HeaderValue;
use hyper::{HeaderMap, header::FORWARDED};
use nailfv::{Parser, extract_for};

const X_REAL_IP: &str = "x-real-ip";
const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// Tries to parse the `x-forwarded-for` header
pub fn maybe_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_FORWARDED_FOR)
        .and_then(header_value_to_str)
        .and_then(|header| {
            header
                .split(',')
                .map(str::trim)
                .filter(|&header_parts| !header_parts.is_empty())
                .find_map(|part| part.parse().ok())
        })
}

/// Tries to parse the `x-real-ip` header
pub fn maybe_x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(X_REAL_IP)
        .and_then(header_value_to_str)
        .and_then(|header| header.trim().parse().ok())
}

/// Tries to parse `forwarded` headers
pub fn maybe_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers.get_all(FORWARDED).iter().find_map(|header| {
        header_value_to_str(header).and_then(|header| {
            header
                .split(&[',', ';'])
                .map(str::trim)
                .filter(|&header_parts| !header_parts.is_empty())
                .find_map(|header_parts| extract_for.parse(header_parts).ok())
        })
    })
}

pub fn header_value_to_str(header_value: &HeaderValue) -> Option<&str> {
    header_value.to_str().ok()
}
