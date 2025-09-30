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
use tokio::time::Sleep;

use crate::{
    delay::delay_output,
    html_gen::{footer, initial_content, main_content},
};

mod delay;
mod html_gen;

static INTERNER: LazyLock<Arc<RwLock<Interner>>> = LazyLock::new(Default::default);

#[derive(Clone)]
pub struct MarkovGen {
    chain: Arc<NailKov>,
}

enum GeneratorState {
    Header,
    MainContent,
    Delay { delay: Pin<Box<Sleep>> },
    Footer,
    Finished,
}

pin_project! {
    pub struct MarkovStream {
        path: NestedPath,
        config: Arc<NailConfig>,
        markov: MarkovGen,
        start_time: Instant,
        total_bytes: usize,
        rng: FastRng,
        state: GeneratorState,
    }
}

impl MarkovStream {
    pub fn new(path: NestedPath, config: Arc<NailConfig>, markov: MarkovGen) -> Self {
        Self {
            path,
            config,
            markov,
            total_bytes: 0,
            start_time: Instant::now(),
            rng: FastRng::default(),
            state: GeneratorState::Header,
        }
    }
}

impl Stream for MarkovStream {
    type Item = Bytes;

    #[fastrace::trace]
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();

        loop {
            match this.state {
                GeneratorState::Header => {
                    let mut content = BytesMut::with_capacity(2048);

                    initial_content(&this.markov.chain, this.config, this.rng, &mut content);

                    *this.total_bytes += content.len();

                    *this.state = GeneratorState::MainContent;

                    return Poll::Ready(Some(content.freeze()));
                }
                GeneratorState::MainContent => {
                    let time_limit = std::time::Duration::from_secs(this.config.generator.timeout);

                    if time_limit.as_secs() == 0 && this.start_time.elapsed() >= time_limit {
                        *this.state = GeneratorState::Footer;
                        continue;
                    }

                    if *this.total_bytes >= (this.config.generator.payload_size * 1024) {
                        *this.state = GeneratorState::Footer;
                        continue;
                    }

                    if let Some(delay) = delay_output(this.config, this.rng) {
                        *this.state = GeneratorState::Delay {
                            delay: Box::pin(delay),
                        };
                        continue;
                    }

                    let content = main_content(&this.markov.chain, this.config, this.rng);

                    *this.total_bytes += content.len();

                    return Poll::Ready(Some(content));
                }
                GeneratorState::Delay { delay } => {
                    if delay.as_mut().poll(cx).is_pending() {
                        return Poll::Pending;
                    }

                    *this.state = GeneratorState::MainContent;
                }
                GeneratorState::Footer => {
                    let content = footer(
                        this.path.as_str(),
                        &this.config.generator.prompts,
                        this.config.generator.max_pit_links,
                        this.rng,
                    );

                    *this.total_bytes += content.len();

                    *this.state = GeneratorState::Finished;

                    log::info!(
                        "Written ({:.2} MB in {}us)",
                        (*this.total_bytes as f64) * 1e-6,
                        this.start_time.elapsed().as_micros()
                    );

                    return Poll::Ready(Some(content));
                }
                GeneratorState::Finished => return Poll::Ready(None),
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
