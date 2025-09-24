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
) -> impl Iterator<Item = &'a str> + 'a {
    chain
        .generate_tokens(rng)
        .flat_map(|token| interner.lookup(token))
        .take(size)
}

pub fn title(chain: &NailKov, config: &NailConfig, rng: &mut impl RngCore) -> (Bytes, Bytes) {
    let interner = INTERNER.read();

    let title_text: String = text_generator(&interner, chain, 24, rng).collect();

    let mut title = BytesMut::new();

    title.extend_from_slice(b"<title>");
    title.extend_from_slice(title_text.as_bytes());
    title.extend_from_slice(b"</title>\n");

    let mut header = BytesMut::new();

    header.extend_from_slice(b"<header><h1>");
    // Consume the title string, so we don't waste the allocated space.
    header.extend(title_text.into_bytes());
    header.extend_from_slice(b"</h1></header>\n");

    // Randomise how many initial paragraphs we want
    let max_paras: u32 = rng.random_range(1..=3);

    for _ in 0..max_paras {
        header.extend(paragraph(
            &interner,
            chain,
            get_desired_size(config, rng),
            rng,
        ));
    }

    (title.freeze(), header.freeze())
}

pub fn paragraph<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a u8> + 'a {
    into_bytes_iter(
        once("<p>")
            .chain(text_generator(interner, chain, size, rng))
            .chain(once("</p>\n")),
    )
}

pub fn header<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
    size: usize,
    rng: &'a mut impl RngCore,
) -> impl Iterator<Item = &'a u8> + 'a {
    into_bytes_iter(
        once("\n<h2>")
            .chain(text_generator(interner, chain, size, rng))
            .chain(once("</h2>\n")),
    )
}

pub fn links<'a>(
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

pub fn footer(route: &str, prompts: &[String], max_links: usize, rng: &mut impl RngCore) -> Bytes {
    let mut footer = BytesMut::with_capacity(2048);

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

#[inline]
fn into_bytes_iter<'a>(generator: impl Iterator<Item = &'a str>) -> impl Iterator<Item = &'a u8> {
    generator.flat_map(|text| text.as_bytes())
}
