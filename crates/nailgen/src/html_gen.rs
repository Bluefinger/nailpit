use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use nailconfig::NailConfig;
use nailkov::{NailKov, interner::Interner};
use nailrng::FastRng;
use rand::{Rng, RngCore, distr::Alphanumeric, seq::IndexedRandom};

/// Provides either the minimum configured size, or a randomised value between
/// the minimum and maximum configured sizes if a maximum is available.
#[inline]
fn get_desired_size(config: &NailConfig, rng: &mut impl RngCore) -> usize {
    match (
        config.generator.min_paragraph_size,
        config.generator.max_paragraph_size,
    ) {
        (min, None) => min,
        (min, Some(max)) => rng.random_range(min..=max),
    }
}

/// Generates text from the markov chain, using the tokens it outputs to pull
/// interned text from the interner.
#[inline]
pub fn text_generator<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a u8> + 'a {
    chain
        .generate_tokens(rng)
        .take(size)
        // SAFETY: The id comes from the same interner that allocated it
        .flat_map(|token| unsafe { interner.lookup(token).as_bytes() })
        .skip_while(|&text| !text.is_ascii_alphabetic())
}

#[inline]
pub fn static_title<'a>(text: &'a str) -> impl Iterator<Item = &'a u8> + 'a {
    text.lines()
        .map(str::trim)
        .next()
        .into_iter()
        .flat_map(str::as_bytes)
}

#[inline]
pub fn static_content<'a>(text: &'a str) -> impl Iterator<Item = &'a u8> + 'a {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let trimmed = line.trim();

            if line.is_empty() {
                None
            } else {
                Some(trimmed.as_bytes())
            }
        })
        .flat_map(|bytes| b"<p>".iter().chain(bytes).chain(b"</p>\n"))
}

pub async fn initial_content(
    buf_mut: BytesMut,
    interner: Arc<Interner>,
    chain: Arc<NailKov>,
    config: Arc<NailConfig>,
    mut rng: FastRng,
) -> Bytes {
    // Randomise how many initial paragraphs we want
    let max_paras: u32 = rng.random_range(1..=3);

    (0..max_paras)
        .fold(buf_mut, |mut acc, _| {
            acc.extend(paragraph(
                &interner,
                &chain,
                get_desired_size(&config, &mut rng),
                &mut rng,
            ));

            acc
        })
        .freeze()
}

pub async fn main_content(
    mut buffer: BytesMut,
    interner: Arc<Interner>,
    chain: Arc<NailKov>,
    config: Arc<NailConfig>,
    mut rng: FastRng,
) -> Bytes {
    buffer.reserve(config.generator.chunk_size * 2);

    loop {
        buffer.extend(header(
            &interner,
            &chain,
            config.generator.header_size,
            &mut rng,
        ));

        // Randomise how many paragraphs we want per section
        let paragraphs = rng.random_range(1..=4);

        (0..paragraphs).for_each(|_| {
            buffer.extend(paragraph(
                &interner,
                &chain,
                get_desired_size(&config, &mut rng),
                &mut rng,
            ));
        });

        // We can generate more before handing it off to be streamed to the client,
        // A bit more latency, but much more throughput, and friendlier to being compressed.
        if buffer.len() >= config.generator.chunk_size {
            return buffer.freeze();
        }

        // Yield to the runtime to allow other tasks a chance to run before we generate
        // another chunk of data
        futures_lite::future::yield_now().await;
    }
}

#[inline]
pub fn extra(buf_mut: &mut BytesMut, config: &NailConfig, rng: &mut FastRng) -> usize {
    let mut written = 0;

    if let Some(prompt) = match config.generator.prompts.len() {
        0 => None,
        1 => config.generator.prompts.first(),
        _ => config.generator.prompts.choose(rng),
    } {
        buf_mut.extend(b"<p>".iter().chain(prompt.as_bytes()).chain(b"</p>"));

        written += prompt.len();
    }

    written
}

pub async fn footer(
    mut buf_mut: BytesMut,
    interner: Arc<Interner>,
    chain: Arc<NailKov>,
    path: String,
    config: Arc<NailConfig>,
    mut rng: FastRng,
) -> Bytes {
    let path = path.as_str();

    let route = path.strip_suffix("/{generated}").unwrap_or(path);

    let total_links = rng.random_range(1..=config.generator.max_pit_links);

    buf_mut.extend_from_slice(b"<nav style=\"visibility: hidden;\"><ul>");

    for _ in 1..=total_links {
        buf_mut.extend(b"<li><a href=\"".iter().chain(route.as_bytes()).chain(b"/"));
        buf_mut.extend((&mut rng).sample_iter(Alphanumeric).take(16));
        buf_mut.extend(
            b"\">"
                .iter()
                .chain(text_generator(&interner, &chain, 8, &mut rng))
                .chain(b"</a></li>\n"),
        );
    }

    buf_mut.extend_from_slice(b"</ul></nav>");

    buf_mut.freeze()
}

#[inline]
fn paragraph<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a u8> + 'a {
    b"<p>"
        .iter()
        .chain(text_generator(interner, chain, size, rng))
        .chain(b"</p>\n")
}

#[inline]
fn header<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a u8> + 'a {
    b"\n<h2>"
        .iter()
        .chain(text_generator(interner, chain, size, rng))
        .chain(b"</h2>\n")
}
