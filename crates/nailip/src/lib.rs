use core::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use hyper::HeaderMap;

pub use crate::maybe_header::*;

mod maybe_header;

#[derive(Debug, Clone, Copy)]
#[repr(align(4))]
pub struct IdentifiedPeer(IpAddr);

impl core::hash::Hash for IdentifiedPeer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self.0 {
            IpAddr::V4(a) => a.to_bits().hash(state),
            IpAddr::V6(aaaa) => aaaa.to_bits().hash(state),
        }
    }
}

impl IdentifiedPeer {
    pub fn extract(headers: &HeaderMap, connection: &ConnectInfo<SocketAddr>) -> Self {
        Self(
            maybe_x_forwarded_for(headers)
                .or_else(|| maybe_x_real_ip(headers))
                .or_else(|| maybe_forwarded_for(headers))
                .unwrap_or_else(|| connection.ip()),
        )
    }

    pub fn ip(&self) -> IpAddr {
        self.0
    }
}

impl core::fmt::Display for IdentifiedPeer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
