use std::{path::Path, sync::Arc};

use bytes::{Bytes, BytesMut};
use color_eyre::Result;
use futures_lite::Stream;
use nailkov::NailKov;
use rand::{Rng, distr::Alphanumeric};
use tokio::sync::mpsc;

use crate::{
    INTERNER,
    html_gen::{header, paragraph, title},
    rng::FastRng,
};

#[derive(Debug, Clone)]
pub struct MarkovGen {
    chain: Arc<NailKov>,
    size: usize,
}

impl MarkovGen {
    #[fastrace::trace]
    pub fn new(size: usize, input: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::read_to_string(input.as_ref())?;

        let mut interner_write_lock = INTERNER.write();

        let chain = Arc::new(NailKov::from_input(&mut interner_write_lock, &file)?);

        drop(interner_write_lock);

        Ok(Self { chain, size })
    }

    #[fastrace::trace]
    fn generate(chain: Arc<NailKov>, desired_size: usize, tx: mpsc::Sender<Bytes>) {
        let mut rng = FastRng::default();

        let mut buffer = BytesMut::new();

        let (title, content) = title(chain.as_ref(), desired_size, &mut rng);

        if tx.blocking_send(title).is_err() {
            return;
        }

        buffer.extend(content);

        loop {
            // We can generate more before handing it off to be streamed to the client,
            // A bit more latency, but much more throughput, and friendlier to being compressed.
            // If the channel errors, then it is closed and we can break out of the loop.
            if buffer.len() >= 4096
                && tx
                    .blocking_send(core::mem::take(&mut buffer).freeze())
                    .is_err()
            {
                break;
            }

            // Randomise how many paragraphs we want per section
            let max_paras: u32 = rng.random_range(1..=4);

            buffer.extend(header(chain.as_ref(), 24, &mut rng));

            for _ in 0..max_paras {
                buffer.extend(paragraph(chain.as_ref(), desired_size, &mut rng));
            }
        }
    }

    #[fastrace::trace]
    pub fn start(&self, tx: mpsc::Sender<Bytes>) {
        let desired_size = self.size.max(128);
        let chain = self.chain.clone();

        tokio::task::spawn_blocking(move || MarkovGen::generate(chain, desired_size, tx));
    }

    #[fastrace::trace]
    async fn spawn_generator(self, tx: mpsc::Sender<Bytes>) {
        let (gen_tx, mut generator) = mpsc::channel(4);
        self.start(gen_tx);
        let mut bytes_written = 0_usize;
        let start_time = std::time::Instant::now();

        // For the first payload we want to make it look like an HTML page.
        // We want to ensure it has a unique title that matches the article header, so to
        // make it look more like a legit page.
        let mut initial_payload = BytesMut::from(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    "#,
        );

        if let Some(title) = generator.recv().await {
            initial_payload.extend(title);
        } else {
            return;
        }

        initial_payload.extend(
            r#"    <meta charset="utf-8" />
    <meta name="robots" content="noindex, nofollow, nosnippet, noimageindex" />
    <meta name="referrer" content="noreferrer">
</head>
<body><main><article>"#
                .bytes(),
        );

        let Some(content) = generator.recv().await else {
            return;
        };

        initial_payload.extend(content);

        let payload_size = initial_payload.len();

        if tx.send(initial_payload.freeze()).await.is_ok() {
            bytes_written += payload_size;
        } else {
            log::info!("Stream broken before first payload could be sent");
            return;
        };

        let time_limit_duration = std::time::Duration::from_secs(60);
        let size_limit = 1024 * 1024;
        loop {
            if time_limit_duration.as_secs() != 0 && (start_time.elapsed() > time_limit_duration) {
                log::info!(
                    "Time limit was reached ({} s), breaking stream",
                    time_limit_duration.as_secs()
                );
                let final_str = BytesMut::from("</article></main>\n</body>\n</html>");

                tx.send(final_str.freeze()).await.ok();
                return;
            }

            let Some(content) = generator.recv().await else {
                return;
            };

            let content_size = content.len();
            if tx.send(content).await.is_ok() {
                bytes_written += content_size;
            } else {
                log::info!(
                    "Stream broken, wrote {:.2} MB",
                    (bytes_written as f64) * 1e-6
                );
                break;
            };

            if size_limit != 0 && bytes_written >= size_limit {
                let mut rng = FastRng::default();

                let link_one = (&mut rng)
                    .sample_iter(Alphanumeric)
                    .take(16)
                    .map(|a| a as char)
                    .collect::<String>();
                let link_two = (&mut rng)
                    .sample_iter(Alphanumeric)
                    .take(16)
                    .map(|a| a as char)
                    .collect::<String>();

                let footer = format!(
                    "</article></main>\n<footer><nav><ul><li><a href=\"/private/{}\">More</a></li><li><a href=\"/private/{}\">Things</a></li></ul></nav></footer></body>\n</html>",
                    link_one, link_two
                );

                let final_str = Bytes::from(footer);

                tx.send(final_str).await.ok();

                log::info!(
                    "Size limit was reached ({:.2} MB in {}us",
                    (bytes_written as f64) * 1e-6,
                    start_time.elapsed().as_micros()
                );
                return;
            }
        }
    }

    #[fastrace::trace]
    pub fn into_stream(self) -> impl Stream<Item = Bytes> {
        let (tx, rx) = mpsc::channel::<Bytes>(4);

        tokio::spawn(self.spawn_generator(tx));

        tokio_stream::wrappers::ReceiverStream::new(rx)
    }
}
