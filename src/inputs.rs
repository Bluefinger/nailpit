use std::sync::Arc;

use glob::glob;

use crate::{config::NailConfig, markov::MarkovGen};

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
