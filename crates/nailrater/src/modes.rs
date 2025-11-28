use std::time::Duration;

use nailconfig::{DropBehavior, RateLimitingConfig};

use crate::PeerState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitModes {
    None,
    Soft {
        limit: u64,
        delay: u64,
    },
    Hard {
        limit: u64,
        spicy: bool,
    },
    SoftHard {
        soft_limit: u64,
        hard_limit: u64,
        delay: u64,
        spicy: bool,
    },
}

impl LimitModes {
    pub(crate) fn limit(&self, visits: &u64) -> PeerState {
        match *self {
            LimitModes::Soft { limit, delay } if (limit..).contains(visits) => {
                PeerState::Delay(Duration::from_millis(delay))
            }
            LimitModes::Hard { limit, spicy } if (limit..).contains(visits) => {
                if spicy {
                    PeerState::SpicyDrop
                } else {
                    PeerState::Drop
                }
            }
            LimitModes::SoftHard {
                soft_limit,
                hard_limit,
                delay,
                ..
            } if (soft_limit..hard_limit).contains(visits) => {
                PeerState::Delay(Duration::from_millis(delay))
            }
            LimitModes::SoftHard {
                hard_limit, spicy, ..
            } if (hard_limit..).contains(visits) => {
                if spicy {
                    PeerState::SpicyDrop
                } else {
                    PeerState::Drop
                }
            }
            _ => PeerState::Ready,
        }
    }
}

impl From<&RateLimitingConfig> for LimitModes {
    #[inline]
    fn from(value: &RateLimitingConfig) -> Self {
        match value {
            RateLimitingConfig::NoLimit => Self::None,
            &RateLimitingConfig::SoftLimit {
                soft_limit: limit,
                soft_delay: delay,
            } => Self::Soft { limit, delay },
            RateLimitingConfig::HardLimit {
                hard_limit: limit,
                drop_behavior,
            } => Self::Hard {
                limit: *limit,
                spicy: matches!(drop_behavior, DropBehavior::Spicy { .. }),
            },
            RateLimitingConfig::SoftWithHardLimit {
                soft_limit,
                hard_limit,
                soft_delay: delay,
                drop_behavior,
            } => Self::SoftHard {
                soft_limit: *soft_limit,
                hard_limit: *hard_limit,
                delay: *delay,
                spicy: matches!(drop_behavior, DropBehavior::Spicy { .. }),
            },
        }
    }
}
