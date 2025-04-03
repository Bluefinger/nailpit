//! At the heart of a [`Chain`](crate::Chain) is a [`Token`]. In fact, this is just a String. But we make a
//! distinction here: A Token is any atomic piece of text.
//!
//! When using [`ChainBuilder::feed_str()`](crate::chain::ChainBuilder::feed_str()),
//! it is the output of [`unicode_segmentation::UnicodeSegmentation::split_word_bounds()`]; that is,
//! it can be a word, a symbol like `"`, or something else. See that crate for more information.
//!
//! If you want more control of what you want a token to be, you can use
//! [`ChainBuilder::feed_tokens()`](crate::chain::ChainBuilder::feed_tokens()).

use hashbrown::Equivalent;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Representation of a string segment.
pub type Token = String;

/// An owned pair of [`Token`]s.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TokenPair(pub Token, pub Token);

/// A borrowed version of [`Token`]; if [`Token`] is [`String`], then [`TokenRef`] is `&str`.
pub type TokenRef<'a> = &'a str;

/// A borrowed version of [`TokenPair`] that does not own its pair. Like [`TokenRef`] to [`Token`].
pub type TokenPairRef<'a> = (TokenRef<'a>, TokenRef<'a>);

impl TokenPair {
    pub fn new(left: &str, right: &str) -> Self {
        Self(left.to_string(), right.to_string())
    }
}

impl From<&TokenPairRef<'_>> for TokenPair {
    fn from(value: &TokenPairRef) -> Self {
        Self(value.0.to_string(), value.1.to_string())
    }
}

impl TokenPair {
    pub fn as_ref(&self) -> TokenPairRef<'_> {
        (&self.0, &self.1)
    }
}

impl PartialEq<&TokenPairRef<'_>> for TokenPair {
    fn eq(&self, other: &&TokenPairRef<'_>) -> bool {
        self.0 == *other.0 && self.1 == *other.1
    }
}

impl PartialEq<TokenPairRef<'_>> for TokenPair {
    fn eq(&self, other: &TokenPairRef<'_>) -> bool {
        self.eq(&other)
    }
}

impl Equivalent<TokenPair> for &TokenPairRef<'_> {
    fn equivalent(&self, key: &TokenPair) -> bool {
        key.eq(self)
    }
}

impl Equivalent<TokenPair> for TokenPairRef<'_> {
    fn equivalent(&self, key: &TokenPair) -> bool {
        key.eq(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::token::TokenPair;

    use super::TokenPairRef;

    #[test]
    fn equivalent_token_pair_with_ref() {
        let tp_ref: TokenPairRef = ("hello", "there");
        let tp = TokenPair::new("hello", "there");
        assert_eq!(tp, tp_ref);
        assert_eq!(tp, &tp_ref);
        assert_eq!(&tp, &tp_ref);
    }
}
