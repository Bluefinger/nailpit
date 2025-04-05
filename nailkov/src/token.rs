use std::ops::Deref;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::interner::InternedString;

/// Representation of a string segment.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(transparent)]
pub struct Token(InternedString);

impl Token {
    pub fn new(ptr: InternedString) -> Self {
        Self(ptr)
    }

    pub fn id(&self) -> InternedString {
        self.0
    }
}

impl From<InternedString> for Token {
    fn from(value: InternedString) -> Self {
        Self::new(value)
    }
}

// impl From<&str> for Token {
//     fn from(value: &str) -> Self {
//         Self(value.into())
//     }
// }

impl Deref for Token {
    type Target = InternedString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// An owned pair of [`Token`]s.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TokenPair(pub Token, pub Token);

/// A borrowed version of [`TokenPair`].
// pub type TokenPairRef<'a> = (&'a str, &'a str);

impl TokenPair {
    pub fn new<T: Into<Token>>(left: T, right: T) -> Self {
        Self(left.into(), right.into())
    }
}

// impl From<&TokenPairRef<'_>> for TokenPair {
//     fn from(value: &TokenPairRef) -> Self {
//         Self(value.0.into(), value.1.into())
//     }
// }

impl AsRef<TokenPair> for TokenPair {
    fn as_ref(&self) -> &TokenPair {
        self
    }
}

// impl TokenPair {
//     pub fn as_token_pair_ref(&self) -> TokenPairRef<'_> {
//         (&self.0, &self.1)
//     }
// }

// impl PartialEq<&TokenPairRef<'_>> for TokenPair {
//     fn eq(&self, other: &&TokenPairRef<'_>) -> bool {
//         self.0.as_ref() == other.0 && self.1.as_ref() == other.1
//     }
// }

// impl PartialEq<TokenPairRef<'_>> for TokenPair {
//     fn eq(&self, other: &TokenPairRef<'_>) -> bool {
//         self.eq(&other)
//     }
// }

// impl Equivalent<TokenPair> for &TokenPairRef<'_> {
//     fn equivalent(&self, key: &TokenPair) -> bool {
//         key.eq(self)
//     }
// }

// impl Equivalent<TokenPair> for TokenPairRef<'_> {
//     fn equivalent(&self, key: &TokenPair) -> bool {
//         key.eq(self)
//     }
// }

// impl Equivalent<Token> for str {
//     fn equivalent(&self, key: &Token) -> bool {
//         key.as_ref().eq(self)
//     }
// }

// #[cfg(test)]
// mod tests {
//     use crate::token::TokenPair;

//     use super::TokenPairRef;

//     #[test]
//     fn equivalent_token_pair_with_ref() {
//         let tp_ref: TokenPairRef = ("hello", "there");
//         let tp = TokenPair::new("hello", "there");
//         assert_eq!(tp, tp_ref);
//         assert_eq!(tp, &tp_ref);
//         assert_eq!(&tp, &tp_ref);
//     }
// }
