use actix_web::dev::ConnectionInfo;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct IdentifiedPeer(String);

impl IdentifiedPeer {
    pub fn extract(connection: &ConnectionInfo) -> Self {
        Self(
            connection
                .realip_remote_addr()
                .or_else(|| connection.peer_addr())
                .unwrap_or("Unknown")
                .into(),
        )
    }

    pub fn peer(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for IdentifiedPeer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
