//! Crate for defining a HTML generator based on a markov chain source, using a string
//! interner to reduce memory usage both within a markov chain and across multiple chains.

use core::{pin::Pin, task::Poll};
use std::{
    path::Path,
    sync::{Arc, LazyLock},
    time::Instant,
};

use axum::extract::NestedPath;
use bytes::{Bytes, BytesMut};
use color_eyre::Result;
use futures_lite::Stream;
use nailconfig::NailConfig;
use nailkov::{NailKov, interner::Interner};
use nailrng::FastRng;
use parking_lot::RwLock;
use pin_project_lite::pin_project;
use rand::{Rng, RngCore};
use tokio::time::Sleep;

use crate::delay::delay_output;
use crate::html_gen::{footer, get_desired_size, header, initial_content, paragraph};

mod delay;
mod html_gen;

static INTERNER: LazyLock<Arc<RwLock<Interner>>> = LazyLock::new(Default::default);

#[derive(Clone)]
pub struct MarkovGen {
    chain: Arc<NailKov>,
}

pin_project! {
    #[project = GeneratorStateProj]
    enum GeneratorState {
        Start,
        Pump,
        Delay {
            delay: Pin<Box<Sleep>>,
        },
        Footer,
        Finished,
    }
}

pin_project! {
    pub struct MarkovStream {
        path: NestedPath,
        config: Arc<NailConfig>,
        chain: MarkovGen,
        start_time: Instant,
        total_bytes: usize,
        rng: FastRng,
        #[pin]
        state: GeneratorState,
    }
}

impl MarkovStream {
    pub fn new(path: NestedPath, config: Arc<NailConfig>, chain: MarkovGen) -> Self {
        Self {
            path,
            config,
            chain,
            total_bytes: 0,
            start_time: Instant::now(),
            rng: FastRng::default(),
            state: GeneratorState::Start,
        }
    }

    #[fastrace::trace]
    fn pump(chain: &NailKov, config: &NailConfig, rng: &mut impl RngCore) -> Bytes {
        // Allocate more than we need, as we might generate more tokens than our 4kB threshold
        let mut buffer = BytesMut::with_capacity(config.generator.chunk_size * 2);

        let interner = INTERNER.read();

        loop {
            // Randomise how many paragraphs we want per section
            let max_paras: u32 = rng.random_range(1..=4);

            buffer.extend(header(&interner, chain, config.generator.header_size, rng));

            for _ in 0..max_paras {
                buffer.extend(paragraph(
                    &interner,
                    chain,
                    get_desired_size(config, rng),
                    rng,
                ));
            }

            // We can generate more before handing it off to be streamed to the client,
            // A bit more latency, but much more throughput, and friendlier to being compressed.
            if buffer.len() >= config.generator.chunk_size {
                return buffer.freeze();
            }
        }
    }
}

impl Stream for MarkovStream {
    type Item = Bytes;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            match this.state.as_mut().project() {
                GeneratorStateProj::Start => {
                    let mut content = BytesMut::with_capacity(2048);

                    initial_content(&this.chain.chain, this.config, this.rng, &mut content);

                    *this.total_bytes += content.len();

                    this.state.set(GeneratorState::Pump);

                    return Poll::Ready(Some(content.freeze()));
                }
                GeneratorStateProj::Pump => {
                    let time_limit = std::time::Duration::from_secs(this.config.generator.timeout);

                    if time_limit.as_secs() == 0 && this.start_time.elapsed() >= time_limit {
                        this.state.set(GeneratorState::Footer);
                        continue;
                    }

                    if *this.total_bytes >= (this.config.generator.payload_size * 1024) {
                        this.state.set(GeneratorState::Footer);
                        continue;
                    }

                    if let Some(delay) = delay_output(this.config, this.rng) {
                        this.state.set(GeneratorState::Delay {
                            delay: Box::pin(delay),
                        });
                        continue;
                    }

                    let content = MarkovStream::pump(&this.chain.chain, this.config, this.rng);

                    *this.total_bytes += content.len();

                    return Poll::Ready(Some(content));
                }
                GeneratorStateProj::Delay { delay } => {
                    if delay.as_mut().poll(cx).is_pending() {
                        return Poll::Pending;
                    }

                    this.state.set(GeneratorState::Pump);
                }
                GeneratorStateProj::Footer => {
                    let content = footer(
                        this.path.as_str(),
                        &this.config.generator.prompts,
                        this.config.generator.max_pit_links,
                        this.rng,
                    );

                    *this.total_bytes += content.len();

                    this.state.set(GeneratorState::Finished);

                    log::info!(
                        "Written ({:.2} MB in {}us)",
                        (*this.total_bytes as f64) * 1e-6,
                        this.start_time.elapsed().as_micros()
                    );

                    return Poll::Ready(Some(content));
                }
                GeneratorStateProj::Finished => return Poll::Ready(None),
            }
        }
    }
}

impl MarkovGen {
    pub fn new(input: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::read_to_string(input.as_ref())?;

        let mut interner_write_lock = INTERNER.write();

        let chain = Arc::new(NailKov::from_input(&mut interner_write_lock, &file)?);

        drop(interner_write_lock);

        Ok(Self { chain })
    }

    #[fastrace::trace]
    pub fn into_stream(self, path: NestedPath, config: Arc<NailConfig>) -> MarkovStream {
        MarkovStream::new(path, config, self)
    }
}
