use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use nailconfig::NailConfig;
use nailkov::{NailKov, interner::Interner};
use nailrng::FastRng;
use rand::{Rng, RngCore, distr::Alphanumeric, seq::IndexedRandom};

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
) -> impl Iterator<Item = &'a u8> + 'a {
    chain
        .generate_tokens(rng)
        .take(size)
        .filter_map(|token| interner.lookup_bytes(token))
        .flatten()
        .skip_while(|&text| !text.is_ascii_alphabetic())
}

#[fastrace::trace(enter_on_poll = true)]
pub async fn initial_content(
    interner: Arc<Interner>,
    chain: Arc<NailKov>,
    config: Arc<NailConfig>,
) -> Bytes {
    let mut buf_mut = BytesMut::with_capacity(2048);
    let mut rng = FastRng::default();
    buf_mut.extend_from_slice(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    "#
        .as_bytes(),
    );

    let title_text: Vec<u8> = text_generator(&interner, &chain, 24, &mut rng)
        .copied()
        .collect();

    buf_mut.extend_from_slice(b"<title>");
    buf_mut.extend_from_slice(title_text.as_slice());
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

    // Allow other tasks to run before we complete the initial content payload
    futures_lite::future::yield_now().await;

    // Randomise how many initial paragraphs we want
    let max_paras: u32 = rng.random_range(1..=3);

    for _ in 0..max_paras {
        buf_mut.extend(paragraph(
            &interner,
            &chain,
            get_desired_size(&config, &mut rng),
            &mut rng,
        ));
    }

    buf_mut.freeze()
}

#[fastrace::trace(enter_on_poll = true)]
pub async fn main_content(
    interner: Arc<Interner>,
    chain: Arc<NailKov>,
    config: Arc<NailConfig>,
) -> Bytes {
    // Allocate more than we need, as we might generate more tokens than our 4kB threshold
    let mut buffer = BytesMut::with_capacity(config.generator.chunk_size * 2);
    let mut rng = FastRng::default();

    loop {
        // Randomise how many paragraphs we want per section
        let max_paras: u32 = rng.random_range(1..=4);

        buffer.extend(header(
            &interner,
            &chain,
            config.generator.header_size,
            &mut rng,
        ));

        for _ in 0..max_paras {
            buffer.extend(paragraph(
                &interner,
                &chain,
                get_desired_size(&config, &mut rng),
                &mut rng,
            ));
        }

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

#[fastrace::trace]
pub fn footer(interner: &Interner, chain: &NailKov, route: &str, config: &NailConfig) -> Bytes {
    let mut rng = FastRng::default();
    let mut footer = BytesMut::with_capacity(512);

    if let Some(prompt) = match config.generator.prompts.len() {
        0 => None,
        1 => config.generator.prompts.first(),
        _ => config.generator.prompts.choose(&mut rng),
    } {
        footer.extend_from_slice(b"<p>");
        footer.extend_from_slice(prompt.as_bytes());
        footer.extend_from_slice(b"</p>");
    }

    footer.extend_from_slice(b"</article></main>\n<footer>");
    links(
        interner,
        chain,
        route,
        config.generator.max_pit_links,
        &mut rng,
        &mut footer,
    );
    footer.extend_from_slice(b"</footer>\n</body>\n</html>");

    footer.freeze()
}

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

fn links<'a>(
    interner: &'a Interner,
    chain: &'a NailKov,
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
        buf_mut.extend(text_generator(interner, chain, 8, rng));
        buf_mut.extend_from_slice(b"</a></li>");
    }

    buf_mut.extend_from_slice(b"</ul></nav>");
}
