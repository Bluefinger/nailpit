use std::time::Duration;

use nailconfig::RateLimitingConfig;

use crate::PeerState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitModes {
    None,
    Soft {
        limit: u64,
        delay: u64,
    },
    Hard {
        limit: u64,
    },
    SoftHard {
        soft_limit: u64,
        hard_limit: u64,
        delay: u64,
    },
}

impl LimitModes {
    pub(crate) fn limit(&self, visits: &u64) -> PeerState {
        match *self {
            LimitModes::Soft { limit, delay } if (limit..).contains(visits) => {
                PeerState::Delay(Duration::from_millis(delay))
            }
            LimitModes::Hard { limit } if (limit..).contains(visits) => PeerState::Drop,
            LimitModes::SoftHard {
                soft_limit,
                hard_limit,
                delay,
            } if (soft_limit..hard_limit).contains(visits) => {
                PeerState::Delay(Duration::from_millis(delay))
            }
            LimitModes::SoftHard { hard_limit, .. } if (hard_limit..).contains(visits) => {
                PeerState::Drop
            }
            _ => PeerState::Ready,
        }
    }
}

impl From<&RateLimitingConfig> for LimitModes {
    #[inline]
    fn from(value: &RateLimitingConfig) -> Self {
        match *value {
            RateLimitingConfig::NoLimit => Self::None,
            RateLimitingConfig::SoftLimit {
                soft_limit: limit,
                soft_delay: delay,
            } => Self::Soft { limit, delay },
            RateLimitingConfig::HardLimit { hard_limit: limit } => Self::Hard { limit },
            RateLimitingConfig::SoftWithHardLimit {
                soft_limit,
                hard_limit,
                soft_delay: delay,
            } => Self::SoftHard {
                soft_limit,
                hard_limit,
                delay,
            },
        }
    }
}
