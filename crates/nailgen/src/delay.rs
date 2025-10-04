use std::time::Duration;

use nailconfig::NailConfig;
use nailrng::FastRng;
use rand::Rng;
use tokio::time::{Sleep, sleep};

/// Apply a delay/sleep based from provided configuration. Either it provides a set
/// minimum delay, or a randomised value sampled from a range of the minimum and maximum
/// configured delays. If the delay value is `0`, this future will poll ready immediately
/// and no sleep will be invoked.
pub fn delay_output(config: &NailConfig) -> Option<Sleep> {
    let delay = match (config.generator.min_delay, config.generator.max_delay) {
        (min_delay, None) => min_delay,
        (min_delay, Some(max_delay)) => FastRng::default().random_range(min_delay..=max_delay),
    };

    if delay > 0 {
        Some(sleep(Duration::from_millis(delay)))
    } else {
        None
    }
}
