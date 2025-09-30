use std::iter::once;

use bytes::{Bytes, BytesMut};
use nailconfig::NailConfig;
use nailkov::{NailKov, interner::Interner};
use rand::{Rng, RngCore, distr::Alphanumeric, seq::IndexedRandom};

use crate::INTERNER;

/// Provides either the minimum configured size, or a randomised value between
/// the minimum and maximum configured sizes if a maximum is available.
pub fn get_desired_size(config: &NailConfig, rng: &mut impl RngCore) -> usize {
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
fn text_generator<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a [u8]> + 'a {
    chain
        .generate_tokens(rng)
        .flat_map(|token| interner.lookup_bytes(token))
        .take(size)
}

#[fastrace::trace]
pub fn initial_content(
    chain: &NailKov,
    config: &NailConfig,
    rng: &mut impl RngCore,
    buf_mut: &mut BytesMut,
) {
    let interner = INTERNER.read();

    buf_mut.extend_from_slice(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    "#
        .as_bytes(),
    );

    let title_text: Vec<u8> = text_generator(&interner, chain, 24, rng)
        .flatten()
        .copied()
        .collect();

    buf_mut.extend_from_slice(b"<title>");
    buf_mut.extend_from_slice(&title_text);
    buf_mut.extend_from_slice(b"</title>\n");

    buf_mut.extend_from_slice(
        r#"    <meta charset="utf-8" />
    <meta name="robots" content="noindex, nofollow, nosnippet, noimageindex" />
    <meta name="referrer" content="noreferrer" />
    <meta name="color-theme" content="dark" />
</head>
<body><main><article>"#
            .as_bytes(),
    );

    buf_mut.extend_from_slice(b"<header><h1>");
    // Consume the title string, so we don't waste the allocated space.
    buf_mut.extend(title_text);
    buf_mut.extend_from_slice(b"</h1></header>\n");

    // Randomise how many initial paragraphs we want
    let max_paras: u32 = rng.random_range(1..=3);

    for _ in 0..max_paras {
        buf_mut.extend(paragraph(
            &interner,
            chain,
            get_desired_size(config, rng),
            rng,
        ));
    }
}

#[fastrace::trace]
pub fn main_content(chain: &NailKov, config: &NailConfig, rng: &mut impl RngCore) -> Bytes {
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

#[fastrace::trace]
pub fn footer(route: &str, prompts: &[String], max_links: usize, rng: &mut impl RngCore) -> Bytes {
    let mut footer = BytesMut::with_capacity(512);

    if let Some(prompt) = match prompts.len() {
        0 => None,
        1 => prompts.first(),
        _ => prompts.choose(rng),
    } {
        footer.extend_from_slice(b"<p>");
        footer.extend_from_slice(prompt.as_bytes());
        footer.extend_from_slice(b"</p>");
    }

    footer.extend_from_slice(b"</article></main>\n<footer>");
    links(route, max_links, rng, &mut footer);
    footer.extend_from_slice(b"</footer>\n</body>\n</html>");

    footer.freeze()
}

fn paragraph<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a u8> + 'a {
    once(b"<p>".as_slice())
        .chain(text_generator(interner, chain, size, rng))
        .chain(once(b"</p>\n".as_slice()))
        .flatten()
}

fn header<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a u8> + 'a {
    once(b"\n<h2>".as_slice())
        .chain(text_generator(interner, chain, size, rng))
        .chain(once(b"</h2>\n".as_slice()))
        .flatten()
}

fn links<'a>(
    route: &str,
    max_links: usize,
    rng: &'a mut impl RngCore,
    buf_mut: &'a mut BytesMut,
) {
    let total_links = rng.random_range(1..=max_links);

    buf_mut.extend_from_slice(b"<nav style=\"visibility: hidden;\"><ul>");

    for _ in 1..=total_links {
        buf_mut.extend_from_slice(b"<li><a href=\"");
        buf_mut.extend_from_slice(route.as_bytes());
        buf_mut.extend_from_slice(b"/");
        buf_mut.extend(rng.sample_iter(Alphanumeric).take(16));
        buf_mut.extend_from_slice(b"\">");
        buf_mut.extend(rng.sample_iter(Alphanumeric).take(4));
        buf_mut.extend_from_slice(b"</a></li>");
    }

    buf_mut.extend_from_slice(b"</ul></nav>");
}
