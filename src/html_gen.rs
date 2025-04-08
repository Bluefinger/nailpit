use std::iter::once;

use bytes::{Bytes, BytesMut};
use nailkov::{interner::Interner, NailKov};
use rand::{Rng, RngCore};

use crate::INTERNER;

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

pub fn title(chain: &NailKov, size: usize, rng: &mut impl RngCore) -> (Bytes, Bytes) {
    let interner = INTERNER.read();

    let title_text: String = text_generator(&interner, chain, 24, rng).collect();

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
        header.extend(paragraph(chain, size, rng));
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

#[inline]
fn iter_into_bytes<'a>(generator: impl Iterator<Item = &'a str>) -> Bytes {
    Bytes::from_iter(generator.flat_map(|text| text.as_bytes()).copied())
}
