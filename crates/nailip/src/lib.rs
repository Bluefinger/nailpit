use core::net::SocketAddr;

use hyper::HeaderMap;

pub use crate::maybe_header::*;

mod maybe_header;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct IdentifiedPeer(Box<str>);

impl IdentifiedPeer {
    #[inline]
    pub fn extract(headers: &HeaderMap, connection: SocketAddr) -> Self {
        Self(
            maybe_x_forwarded_for(headers)
                .or_else(|| maybe_x_real_ip(headers))
                .or_else(|| maybe_forwarded_for(headers))
                .unwrap_or_else(|| connection.to_string().into()),
        )
    }

    #[inline]
    pub fn peer(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for IdentifiedPeer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
