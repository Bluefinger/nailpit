use std::sync::Arc;

use actix_web::{
    http::header::{ACCEPT_ENCODING, HeaderMap},
    web::Bytes,
};
use hashbrown::HashMap;

use nailbox::try_arc_within;
use nailconfig::{DropBehavior, NailConfig, RateLimitingConfig};
use rapidhash::fast::RandomState;

pub type SpicyPayloads = HashMap<SpicyPayloadKind, Bytes, RandomState>;

static GZIP: &str = "gzip";
static BROTLI: &str = "br";

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
#[repr(u32)]
pub enum SpicyPayloadKind {
    Gz,
    Brotli,
}

impl SpicyPayloadKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpicyPayloadKind::Gz => GZIP,
            SpicyPayloadKind::Brotli => BROTLI,
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
                    .contains(BROTLI)
                    .then_some(Self::Brotli)
                    .or_else(|| header.contains(GZIP).then_some(Self::Gz))
            })
    }
}

pub fn get_spicy_payload(config: &NailConfig) -> Option<Arc<SpicyPayloads>> {
    match &config.rate_limiting {
        RateLimitingConfig::HardLimit {
            drop_behavior: DropBehavior::Spicy { payload },
            ..
        }
        | RateLimitingConfig::SoftWithHardLimit {
            drop_behavior: DropBehavior::Spicy { payload },
            ..
        } => try_arc_within(|| {
            payload
                .iter()
                .filter_map(|file| SpicyPayloadKind::file_kind(file).map(|kind| (kind, file)))
                .map(|(kind, file)| {
                    Ok((
                        kind,
                        std::fs::read(file)
                            .inspect_err(|err| {
                                tracing::error!("Failed to load spicy payload: {err}")
                            })
                            .map(Bytes::from)?,
                    ))
                })
                .collect::<std::io::Result<_>>()
        })
        .ok(),
        _ => None,
    }
}
