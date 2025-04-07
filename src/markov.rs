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
use tokio::sync::mpsc;

use crate::rng::FastRng;

static INTERNER: LazyLock<Arc<RwLock<Interner>>> = LazyLock::new(Default::default);

#[derive(Debug, Clone)]
pub struct MarkovGen {
    chain: Arc<NailKov>,
    size: usize,
}

impl MarkovGen {
    pub fn new(size: usize, input: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::read_to_string(input.as_ref())?;

        let mut interner = INTERNER.write();

        let chain = Arc::new(NailKov::from_input(&mut interner, &file)?);

        drop(interner);

        Ok(Self { chain, size })
    }

    pub fn generate(&self, tx: tokio::sync::mpsc::Sender<Bytes>) {
        let desired_size = self.size.max(128);
        let chain = self.chain.clone();

        tokio::task::spawn_blocking(move || {
            let mut rng_source = FastRng::default();

            loop {
                if tx.is_closed() {
                    break;
                }

                let interner = INTERNER.read();

                let generated = chain
                    .generate_tokens(&mut rng_source)
                    .flat_map(|token| interner.lookup(token))
                    .take(desired_size);

                let final_str = once("<p>\n")
                    .chain(generated)
                    .chain(once("\n</p>\n"))
                    .flat_map(|str| str.as_bytes())
                    .copied();

                if tx.blocking_send(Bytes::from_iter(final_str)).is_err() {
                    break;
                }
            }
        });
    }

    fn into_receiver(self) -> mpsc::Receiver<Bytes> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);
        tokio::spawn(async move {
            let (gen_tx, mut generator) = mpsc::channel(32);
            self.generate(gen_tx);
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
