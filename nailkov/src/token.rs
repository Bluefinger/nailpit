use std::ops::Deref;

use hashbrown::Equivalent;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Representation of a string segment.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(transparent)]
pub struct Token(Box<str>);

impl From<&str> for Token {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl Deref for Token {
    type Target = Box<str>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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
        Self(left.into(), right.into())
    }
}

impl From<&TokenPairRef<'_>> for TokenPair {
    fn from(value: &TokenPairRef) -> Self {
        Self(value.0.into(), value.1.into())
    }
}

impl<'a> AsRef<TokenPair> for TokenPair {
    fn as_ref(&self) -> &TokenPair {
        self
    }
}

impl TokenPair {
    pub fn as_token_pair_ref(&self) -> TokenPairRef<'_> {
        (&self.0, &self.1)
    }
}

impl PartialEq<&TokenPairRef<'_>> for TokenPair {
    fn eq(&self, other: &&TokenPairRef<'_>) -> bool {
        self.0.as_ref() == other.0 && self.1.as_ref() == other.1
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

impl Equivalent<Token> for str {
    fn equivalent(&self, key: &Token) -> bool {
        key.as_ref() == self
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
