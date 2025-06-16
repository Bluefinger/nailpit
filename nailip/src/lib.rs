use std::net::{IpAddr, SocketAddr};

use axum::{
    extract::{ConnectInfo, Request},
    middleware::Next,
    response::Response,
};
use hyper::HeaderMap;

use crate::maybe_header::{maybe_forwarded, maybe_x_forwarded_for, maybe_x_real_ip};

mod maybe_header;

#[derive(Debug, Clone, Copy)]
#[repr(align(8))]
pub struct IdentifiedPeer(IpAddr);

impl IdentifiedPeer {
    fn extract(headers: &HeaderMap, connection: &ConnectInfo<SocketAddr>) -> Self {
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

impl std::fmt::Display for IdentifiedPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub async fn identify_peer(
    connection: ConnectInfo<SocketAddr>,
    mut req: Request,
    next: Next,
) -> Response {
    let extracted = IdentifiedPeer::extract(req.headers(), &connection);

    req.extensions_mut().insert(extracted);

    next.run(req).await
}
