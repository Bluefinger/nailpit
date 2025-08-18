use hyper::{HeaderMap, body::Bytes, header::ACCEPT_ENCODING};
use nailconfig::{DropBehavior, NailConfig, RateLimitingConfig};
use scc::HashIndex;
use wyrand::RandomWyHashState;

pub type SpicyPayloads = HashIndex<SpicyPayloadKind, Bytes, RandomWyHashState>;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum SpicyPayloadKind {
    Gz,
    Brotli,
}

impl SpicyPayloadKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpicyPayloadKind::Gz => "gzip",
            SpicyPayloadKind::Brotli => "br",
        }
    }
}

impl SpicyPayloadKind {
    fn file_kind(file: impl AsRef<str>) -> Option<SpicyPayloadKind> {
        let file = file.as_ref();

        if file.ends_with(".gz") {
            Some(Self::Gz)
        } else if file.ends_with(".br") {
            Some(Self::Brotli)
        } else {
            None
        }
    }

    pub fn accepts_encoding(header: &HeaderMap) -> Option<SpicyPayloadKind> {
        header
            .get(ACCEPT_ENCODING)
            .and_then(|header| header.to_str().ok())
            .and_then(|header| {
                header
                    .contains("br")
                    .then_some(Self::Brotli)
                    .or_else(|| header.contains("gzip").then_some(Self::Gz))
            })
    }
}

pub fn get_spicy_payload(config: &NailConfig) -> Option<SpicyPayloads> {
    match &config.rate_limiting {
        RateLimitingConfig::HardLimit {
            drop_behavior: DropBehavior::Spicy { payload },
            ..
        }
        | RateLimitingConfig::SoftWithHardLimit {
            drop_behavior: DropBehavior::Spicy { payload },
            ..
        } => payload
            .iter()
            .filter_map(|file| SpicyPayloadKind::file_kind(file).map(|kind| (kind, file)))
            .map(|(kind, file)| {
                Some((
                    kind,
                    std::fs::read(file)
                        .inspect_err(|err| log::error!("Failed to load spicy payload: {err}"))
                        .map(Bytes::from)
                        .ok()?,
                ))
            })
            .collect(),
        _ => None,
    }
}
