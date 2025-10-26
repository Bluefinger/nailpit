//! Crate for defining a HTML generator based on a markov chain source, using a string
//! interner to reduce memory usage both within a markov chain and across multiple chains.

use core::task::Poll;
use std::{
    path::Path,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::extract::MatchedPath;
use bytes::{Bytes, BytesMut};
use color_eyre::Result;
use futures_lite::Stream;
use nailbox::{boxed_future_within, try_arc_within};
use nailconfig::NailConfig;
use nailkov::{NailKov, interner::Interner};
use nailrng::FastRng;
use pin_project_lite::pin_project;
use tokio::time::Sleep;

use crate::{
    delay::delay_output,
    html_gen::{
        extra, footer, initial_content, main_content, static_content, static_title, text_generator,
    },
};

pub use crate::template::*;

mod delay;
mod html_gen;
mod template;

#[derive(Clone)]
pub struct MarkovGen {
    chain: Arc<NailKov>,
}

pin_project! {
    #[project = GeneratorStateProj]
    enum GeneratorState {
        Template,
        Content,
        GeneratingContent {
            handle: Pin<Box<dyn Future<Output = Bytes> + Send>>,
            keep_generating: bool,
        },
        Delay {
            delay: Pin<Box<Sleep>>
        },
        Finished,
    }
}

pin_project! {
    pub struct MarkovStream {
        path: MatchedPath,
        config: Arc<NailConfig>,
        interner: Arc<Interner>,
        markov: MarkovGen,
        start_time: Instant,
        total_bytes: usize,
        template: Template,
        cursor: TemplateCursor,
        page_title: Option<Box<[u8]>>,
        rng: FastRng,
        #[pin]
        state: GeneratorState,
    }
}

impl MarkovStream {
    pub fn new(
        markov: MarkovGen,
        path: MatchedPath,
        config: Arc<NailConfig>,
        interner: Arc<Interner>,
        template: Template,
        rng: FastRng,
    ) -> Self {
        Self {
            path,
            config,
            interner,
            markov,
            total_bytes: 0,
            start_time: Instant::now(),
            state: GeneratorState::Template,
            cursor: TemplateCursor::new(template.get_template()),
            rng,
            template,
            page_title: None,
        }
    }
}

impl Stream for MarkovStream {
    type Item = Bytes;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        'outer: loop {
            let mut buffer = BytesMut::new();

            match this.state.as_mut().project() {
                GeneratorStateProj::Template => 'inner: loop {
                    match this.cursor.write_template(&mut buffer) {
                        template::TemplateState::Title => {
                            let title = this.page_title.get_or_insert_with(|| {
                                this.template.get_static_content().map_or_else(
                                    || {
                                        text_generator(
                                            this.interner,
                                            &this.markov.chain,
                                            24,
                                            this.rng,
                                        )
                                        .copied()
                                        .collect()
                                    },
                                    |title| static_title(title).copied().collect(),
                                )
                            });

                            *this.total_bytes += title.len();

                            buffer.extend_from_slice(title);

                            continue 'inner;
                        }
                        template::TemplateState::Initial => {
                            let handle = boxed_future_within(|| {
                                initial_content(
                                    buffer,
                                    this.interner.clone(),
                                    this.markov.chain.clone(),
                                    this.config.clone(),
                                    this.rng.fork(),
                                )
                            });

                            this.state.set(GeneratorState::GeneratingContent {
                                handle,
                                keep_generating: false,
                            });

                            continue 'outer;
                        }
                        template::TemplateState::Main => {
                            if let Some(content) = this.template.get_static_content() {
                                let len = buffer.len();

                                buffer.extend(static_content(content));

                                this.state.set(GeneratorState::Template);

                                let len = buffer.len() - len;

                                *this.total_bytes += len;

                                continue 'inner;
                            } else {
                                this.state.set(GeneratorState::Content);

                                continue 'outer;
                            }
                        }
                        template::TemplateState::Extra => {
                            let bytes = extra(&mut buffer, this.config, this.rng);

                            *this.total_bytes += bytes;

                            continue 'inner;
                        }
                        template::TemplateState::Footer => {
                            let handle = boxed_future_within(|| {
                                footer(
                                    buffer,
                                    this.interner.clone(),
                                    this.markov.chain.clone(),
                                    this.path.clone(),
                                    this.config.clone(),
                                    this.rng.fork(),
                                )
                            });

                            this.state.set(GeneratorState::GeneratingContent {
                                handle,
                                keep_generating: false,
                            });

                            continue 'outer;
                        }
                        template::TemplateState::Finished => {
                            let elapsed_time = this.start_time.elapsed().as_micros();

                            tracing::trace!(
                                "payload.size" = *this.total_bytes,
                                "duration.us" = elapsed_time,
                                "Stream finished in {:.2}ms", (elapsed_time as f32) * 1e-3
                            );

                            this.state.set(GeneratorState::Finished);
                            continue 'outer;
                        }
                    }
                },
                GeneratorStateProj::Content => {
                    let time_limit = Duration::from_secs(this.config.generator.timeout);

                    if time_limit.as_secs() > 0
                        && this.start_time.elapsed().as_secs() >= time_limit.as_secs()
                    {
                        this.state.set(GeneratorState::Template);
                        continue 'outer;
                    }

                    if *this.total_bytes >= (this.config.generator.payload_size * 1024) {
                        this.state.set(GeneratorState::Template);
                        continue 'outer;
                    }

                    let handle = boxed_future_within(|| {
                        main_content(
                            buffer,
                            this.interner.clone(),
                            this.markov.chain.clone(),
                            this.config.clone(),
                            this.rng.fork(),
                        )
                    });

                    this.state.set(GeneratorState::GeneratingContent {
                        handle,
                        keep_generating: true,
                    });
                }
                GeneratorStateProj::GeneratingContent {
                    handle,
                    keep_generating,
                } => match handle.as_mut().poll(cx) {
                    Poll::Ready(content) => {
                        *this.total_bytes += content.len();

                        if let Some(delay) = delay_output(this.config) {
                            this.state.set(GeneratorState::Delay { delay });
                        } else if *keep_generating {
                            this.state.set(GeneratorState::Content);
                        } else {
                            this.state.set(GeneratorState::Template);
                        }

                        return Poll::Ready(Some(content));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                GeneratorStateProj::Delay { delay } => match delay.as_mut().poll(cx) {
                    Poll::Ready(_) => {
                        this.state.set(GeneratorState::Content);
                        continue;
                    }
                    Poll::Pending => return Poll::Pending,
                },
                GeneratorStateProj::Finished => {
                    return Poll::Ready(None);
                }
            }
        }
    }
}

impl MarkovGen {
    pub fn new(input: impl AsRef<Path>, interner: &mut Interner) -> Result<Self> {
        let file = std::fs::read_to_string(input.as_ref())?;

        let chain = try_arc_within(|| NailKov::from_input(interner, &file))?;

        Ok(Self { chain })
    }

    #[inline]
    pub fn into_stream(
        self,
        path: MatchedPath,
        config: Arc<NailConfig>,
        interner: Arc<Interner>,
        template: Template,
        rng: FastRng,
    ) -> MarkovStream {
        MarkovStream::new(self, path, config, interner, template, rng)
    }
}
