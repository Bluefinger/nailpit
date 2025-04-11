use std::ops::Deref;

use crate::interner::InternedString;

/// Representation of a string segment.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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

impl From<Token> for InternedString {
    fn from(value: Token) -> Self {
        value.id()
    }
}

impl Deref for Token {
    type Target = InternedString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// An owned pair of [`Token`]s.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TokenPair(pub Token, pub Token);

impl TokenPair {
    pub fn new<T: Into<Token>>(left: T, right: T) -> Self {
        Self(left.into(), right.into())
    }
}

impl AsRef<TokenPair> for TokenPair {
    fn as_ref(&self) -> &TokenPair {
        self
    }
}
