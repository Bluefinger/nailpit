use std::{iter::once, path::Path, sync::Arc};

use bytes::Bytes;
use color_eyre::eyre::Result;
use markovish::Chain;

use crate::rng::FastRng;

#[derive(Debug, Clone)]
pub struct MarkovGen {
    chain: Arc<Chain>,
    size: usize,
}

impl MarkovGen {
    pub fn new(size: usize, input: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::read_to_string(input.as_ref())?;

        let chain = Arc::new(
            Chain::from_text(&file)
                .map_err(|_| color_eyre::eyre::eyre!("Couldn't create the markov chain"))?,
        );

        Ok(Self { chain, size })
    }

    pub fn generate(&self, tx: tokio::sync::mpsc::Sender<Bytes>) {
        let desired_size = self.size.max(128);
        let chain = self.chain.clone();

        tokio::task::spawn_blocking(move || {
            let mut rng_source = FastRng::default();

            loop {
                let mut current_size = 0;

                if tx.is_closed() {
                    break;
                }

                let final_str = loop {
                    let Some(generated) = chain.generate_str(&mut rng_source, desired_size) else {
                        log::error!("failed to generate string from chain");
                        continue;
                    };

                    let generated = generated.into_iter().take_while(|&s| {
                        current_size += s.len();

                        current_size < desired_size
                    });

                    break once("<p>\n")
                        .chain(generated)
                        .chain(once("\n</p>\n"))
                        .flat_map(|str| str.as_bytes())
                        .copied();
                };

                if tx.blocking_send(Bytes::from_iter(final_str)).is_err() {
                    break;
                }
            }
        });
    }
}
