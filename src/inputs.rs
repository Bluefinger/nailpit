use std::sync::Arc;

use axum::body::Bytes;
use glob::glob;
use nailconfig::{DropBehavior, NailConfig, RateLimitingConfig};
use nailgen::MarkovGen;

/// Takes a glob for finding all input files and returns a read-only list of
/// all markov chains that can be generated.
pub fn get_input_files(config: &NailConfig) -> color_eyre::Result<Arc<[MarkovGen]>> {
    let inputs = glob(&config.generator.input_files)?
        .filter_map(|path| path.inspect_err(|err| log::error!("IO Error: {err}")).ok())
        .filter_map(|input| {
            MarkovGen::new(input)
                .inspect_err(|err| log::error!("Markov Error: {err}"))
                .ok()
        })
        .collect::<Arc<[MarkovGen]>>();

    if inputs.is_empty() {
        color_eyre::eyre::bail!("No input files found! Exiting...");
    }

    Ok(inputs)
}

pub fn get_spicy_payload(config: &NailConfig) -> color_eyre::Result<Option<Bytes>> {
    match &config.rate_limiting {
        RateLimitingConfig::HardLimit {
            drop_behavior: DropBehavior::Spicy { payload },
            ..
        }
        | RateLimitingConfig::SoftWithHardLimit {
            drop_behavior: DropBehavior::Spicy { payload },
            ..
        } => {
            let spicy = std::fs::read(payload)?;

            Ok(Some(Bytes::from(spicy)))
        }
        _ => Ok(None),
    }
}
