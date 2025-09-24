//! Crate for defining a HTML generator based on a markov chain source, using a string
//! interner to reduce memory usage both within a markov chain and across multiple chains.

use std::{
    path::Path,
    sync::{Arc, LazyLock},
};

use axum::extract::NestedPath;
use bytes::{Bytes, BytesMut};
use color_eyre::Result;
use fastrace::{Span, future::FutureExt};
use nailconfig::NailConfig;
use nailkov::{NailKov, interner::Interner};
use nailrng::FastRng;
use parking_lot::RwLock;
use rand::{Rng, RngCore};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::delay::delay_output;
use crate::html_gen::{footer, get_desired_size, header, paragraph, title};

mod delay;
mod html_gen;

static INTERNER: LazyLock<Arc<RwLock<Interner>>> = LazyLock::new(Default::default);

#[derive(Clone)]
pub struct MarkovGen {
    chain: Arc<NailKov>,
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
    fn generate(chain: &NailKov, config: &NailConfig, rng: &mut impl RngCore) -> Bytes {
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

    #[fastrace::trace(enter_on_poll = true)]
    async fn spawn_generator(
        self,
        path: NestedPath,
        config: Arc<NailConfig>,
        tx: mpsc::Sender<Bytes>,
    ) {
        let mut bytes_written = 0_usize;
        let start_time = std::time::Instant::now();
        let mut rng = FastRng::default();

        let (title, content) = title(self.chain.as_ref(), &config, &mut rng);

        let mut initial_payload = BytesMut::with_capacity(2048);

        // For the first payload we want to make it look like an HTML page.
        // We want to ensure it has a unique title that matches the article header, so to
        // make it look more like a legit page.
        initial_payload.extend_from_slice(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    "#
            .as_bytes(),
        );

        initial_payload.extend(title);

        initial_payload.extend_from_slice(
            r#"    <meta charset="utf-8" />
    <meta name="robots" content="noindex, nofollow, nosnippet, noimageindex" />
    <meta name="referrer" content="noreferrer" />
    <meta name="color-theme" content="dark" />
</head>
<body><main><article>"#
                .as_bytes(),
        );

        initial_payload.extend(content);

        let payload_size = initial_payload.len();

        if tx.send(initial_payload.freeze()).await.is_ok() {
            bytes_written += payload_size;
        } else {
            log::info!("Stream broken before first payload could be sent");
            return;
        };

        let time_limit_duration = std::time::Duration::from_secs(config.generator.timeout);
        let size_limit = 1024 * config.generator.payload_size;
        loop {
            delay_output(&config, &mut rng).await;

            if time_limit_duration.as_secs() != 0 && (start_time.elapsed() > time_limit_duration) {
                log::info!(
                    "Time limit was reached ({} s), breaking stream",
                    time_limit_duration.as_secs()
                );
                break;
            }

            let content = MarkovGen::generate(self.chain.as_ref(), &config, &mut rng);

            let content_size = content.len();

            if tx.send(content).await.is_ok() {
                bytes_written += content_size;
            } else {
                log::info!(
                    "Stream broken, wrote {:.2} MB",
                    (bytes_written as f64) * 1e-6
                );
                return;
            };

            if size_limit != 0 && bytes_written >= size_limit {
                log::info!(
                    "Size limit was reached ({:.2} MB in {}us)",
                    (bytes_written as f64) * 1e-6,
                    start_time.elapsed().as_micros()
                );
                break;
            }
        }

        let final_str = footer(
            path.as_str(),
            &config.generator.prompts,
            config.generator.max_pit_links,
            &mut rng,
        );

        tx.send(final_str).await.ok();
    }

    #[fastrace::trace]
    pub fn into_stream(self, path: NestedPath, config: Arc<NailConfig>) -> ReceiverStream<Bytes> {
        let (tx, rx) = mpsc::channel::<Bytes>(8);

        tokio::spawn(
            self.spawn_generator(path, config, tx)
                .in_span(Span::enter_with_local_parent("MarkovGen")),
        );

        ReceiverStream::new(rx)
    }
}
