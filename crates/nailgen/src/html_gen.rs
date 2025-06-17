use std::iter::once;

use bytes::{Bytes, BytesMut};
use nailconfig::NailConfig;
use nailkov::{NailKov, interner::Interner};
use rand::{Rng, RngCore, distr::Alphanumeric};
use std::iter::repeat_with;

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

    drop(interner);

    let mut title = BytesMut::new();

    title.extend(b"<title>");
    title.extend(title_text.bytes());
    title.extend(b"</title>\n");

    let mut header = BytesMut::new();

    header.extend(b"<header><h1>");
    // Consume the title string, so we don't waste the allocated space.
    header.extend(title_text.into_bytes());
    header.extend(b"</h1></header>\n");

    // Randomise how many initial paragraphs we want
    let max_paras: u32 = rng.random_range(1..=3);

    for _ in 0..max_paras {
        header.extend(paragraph(chain, get_desired_size(config, rng), rng));
    }

    (title.freeze(), header.freeze())
}

pub fn paragraph(chain: &NailKov, size: usize, rng: &mut impl RngCore) -> Bytes {
    let interner = INTERNER.read();

    iter_into_bytes(
        once("<p>")
            .chain(text_generator(&interner, chain, size, rng))
            .chain(once("</p>\n")),
    )
}

pub fn header(chain: &NailKov, size: usize, rng: &mut impl RngCore) -> Bytes {
    let interner = INTERNER.read();

    iter_into_bytes(
        once("\n<h2>")
            .chain(text_generator(&interner, chain, size, rng))
            .chain(once("</h2>\n")),
    )
}

pub fn links(route: &str, max_links: usize, rng: &mut impl RngCore) -> Bytes {
    let total_links = rng.random_range(1..=max_links);

    let link = repeat_with(|| {
        rng.sample_iter(Alphanumeric)
            .take(16)
            .map(|a| a as char)
            .collect::<String>()
    })
    .enumerate();

    let mut nav = String::from("<nav><ul>");

    for (i, link) in link.take(total_links) {
        let link = format!("<li><a href=\"{route}/{}\">{}</a></li>", link, i + 1);
        nav = [nav, link].concat();
    }

    nav.push_str("</ul></nav>");

    Bytes::from(nav)
}

pub fn footer(route: &str, max_links: usize, rng: &mut impl RngCore) -> Bytes {
    let mut footer = BytesMut::from("</article></main>\n<footer>");

    footer.extend(links(route, max_links, rng));
    footer.extend(b"</footer>\n</body>\n</html>");

    footer.freeze()
}

#[inline]
fn iter_into_bytes<'a>(generator: impl Iterator<Item = &'a str>) -> Bytes {
    Bytes::from_iter(generator.flat_map(|text| text.as_bytes()).copied())
}
