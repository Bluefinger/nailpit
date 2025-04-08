use std::{
    iter::once,
    path::Path,
    sync::{Arc, LazyLock},
};

use bytes::{Bytes, BytesMut};
use color_eyre::Result;
use futures_lite::Stream;
use nailkov::{NailKov, interner::Interner};
use parking_lot::RwLock;
use rand::{Rng, RngCore};
use tokio::sync::mpsc;

use crate::rng::FastRng;

static INTERNER: LazyLock<Arc<RwLock<Interner>>> = LazyLock::new(Default::default);

#[derive(Debug, Clone)]
pub struct MarkovGen {
    chain: Arc<NailKov>,
    size: usize,
}

fn generator<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a str> + 'a {
    chain
        .generate_tokens(rng)
        .flat_map(|token| interner.lookup(token))
        .take(size)
}

fn paragraph(chain: &NailKov, size: usize, rng: &mut impl RngCore) -> Bytes {
    let interner = INTERNER.read();

    iter_to_bytes(
        once("<p>\n")
            .chain(generator(&interner, chain, size, rng))
            .chain(once("\n</p>\n")),
    )
}

fn h1(chain: &NailKov, size: usize, rng: &mut impl RngCore) -> Bytes {
    let interner = INTERNER.read();

    iter_to_bytes(
        once("\n<h1>")
            .chain(generator(&interner, chain, size, rng))
            .chain(once("</h1>\n")),
    )
}

#[inline]
fn iter_to_bytes<'a>(generator: impl Iterator<Item = &'a str>) -> Bytes {
    Bytes::from_iter(generator.flat_map(|text| text.as_bytes()).copied())
}

impl MarkovGen {
    pub fn new(size: usize, input: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::read_to_string(input.as_ref())?;

        let mut interner = INTERNER.write();

        let chain = Arc::new(NailKov::from_input(&mut interner, &file)?);

        drop(interner);

        Ok(Self { chain, size })
    }

    pub fn start(&self, tx: tokio::sync::mpsc::Sender<Bytes>) {
        let desired_size = self.size.max(128);
        let chain = self.chain.clone();

        tokio::task::spawn_blocking(move || {
            let mut rng = FastRng::default();

            loop {
                if tx.is_closed() {
                    break;
                }

                let max_paras: u32 = rng.random_range(1..3);

                let mut buffer = BytesMut::new();

                buffer.extend(h1(chain.as_ref(), 24, &mut rng));

                for _ in 0..max_paras {
                    buffer.extend(paragraph(chain.as_ref(), desired_size, &mut rng));
                }

                if tx.blocking_send(buffer.freeze()).is_err() {
                    break;
                }
            }
        });
    }

    fn into_receiver(self) -> mpsc::Receiver<Bytes> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);
        tokio::spawn(async move {
            let (gen_tx, mut generator) = mpsc::channel(32);
            self.start(gen_tx);
            let mut bytes_written = 0_usize;

            // For the first value we want to prepend something to make it look like HTML.
            // We don't want to just chain it, because then the first chunk of the body always
            // looks the same.
            let mut first_msg = BytesMut::from(
                r#"<!DOCTYPE html>
<html lang="en">

<head>
    <title>AAAAAAAAAAA (You aren't supposed to be here)</title>
    <meta charset="utf-8" />
    <meta name="robots" content="noindex, nofollow, nosnippet, noimageindex" />
    <meta name="referrer" content="noreferrer">
</head>

<body>
    <main>"#,
            );
            if let Some(first_gen) = generator.recv().await {
                first_msg.extend(first_gen);
            } else {
                return;
            }

            let first_msg_size = first_msg.len();
            let start_time = std::time::SystemTime::now();
            if tx.send(first_msg.freeze()).await.is_ok() {
                bytes_written += first_msg_size;
            } else {
                log::info!("Stream broken before first message could be sent");
                return;
            };

            // Don't want to call `self.config()` over and over
            let time_limit = 60;
            let time_limit_duration = std::time::Duration::from_secs(60);
            let size_limit = 1024 * 1024;
            loop {
                // `0` means no limit

                // If system time is messed up, assume no time has passed
                if time_limit != 0
                    && (start_time
                        .elapsed()
                        .unwrap_or(std::time::Duration::from_secs(0))
                        > time_limit_duration)
                {
                    log::info!("Time limit was reached ({} s), breaking stream", time_limit,);
                    return;
                }

                if size_limit != 0 && bytes_written >= size_limit {
                    log::info!(
                        "Size limit was reached ({:.2} MB, {:.2} GB) in {}us",
                        (bytes_written as f64) * 1e-6,
                        (bytes_written as f64) * 1e-9,
                        start_time
                            .elapsed()
                            .unwrap_or(std::time::Duration::from_secs(0))
                            .as_micros()
                    );
                    return;
                }

                // Limits were find, produce some data
                let Some(s) = generator.recv().await else {
                    return;
                };

                // The size may be dynamic if the generator does not have a strict
                // chunk size
                let s_size = s.len();
                if tx.send(s).await.is_ok() {
                    bytes_written += s_size;
                } else {
                    log::info!(
                        "Stream broken, wrote {:.2} MB, or {:.2} GB",
                        (bytes_written as f64) * 1e-6,
                        (bytes_written as f64) * 1e-9
                    );
                    break;
                };
            }
        });
        rx
    }

    pub fn into_stream(self) -> impl Stream<Item = Bytes> {
        tokio_stream::wrappers::ReceiverStream::new(self.into_receiver())
    }
}
