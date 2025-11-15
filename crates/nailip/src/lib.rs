use core::net::{IpAddr, SocketAddr};

use hyper::HeaderMap;

pub use crate::maybe_header::*;

mod maybe_header;

#[derive(Debug, Clone, Copy, Hash)]
#[repr(align(4))]
pub struct IdentifiedPeer(SocketAddr);

impl IdentifiedPeer {
    #[inline]
    pub fn extract(headers: &HeaderMap, connection: SocketAddr) -> Self {
        Self(SocketAddr::new(
            maybe_x_forwarded_for(headers)
                .or_else(|| maybe_x_real_ip(headers))
                .or_else(|| maybe_forwarded_for(headers))
                .unwrap_or_else(|| connection.ip()),
            connection.port(),
        ))
    }

    #[inline]
    pub fn ip(&self) -> IpAddr {
        self.0.ip()
    }

    #[inline]
    pub fn port(&self) -> u16 {
        self.0.port()
    }
}

impl core::fmt::Display for IdentifiedPeer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
