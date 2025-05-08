use std::ops::Deref;

use crate::interner::InternedString;

/// Representation of a string segment.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Token(InternedString);

impl Token {
    #[inline(always)]
    pub const fn new(ptr: InternedString) -> Self {
        Self(ptr)
    }

    #[inline(always)]
    pub const fn id(&self) -> InternedString {
        self.0
    }

    #[inline(always)]
    pub const fn to_bits(self) -> u32 {
        self.0.to_bits()
    }
}

impl From<InternedString> for Token {
    #[inline]
    fn from(value: InternedString) -> Self {
        Self::new(value)
    }
}

impl From<Token> for InternedString {
    #[inline]
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
#[derive(Copy, Clone, Debug)]
// Alignment repr necessary to allow LLVM to better output
// optimized codegen for `to_bits`, `PartialEq`
// Prior art taken from my contribution to Bevy:
// https://github.com/bevyengine/bevy/blob/main/crates/bevy_ecs/src/entity/mod.rs#L309
#[repr(C, align(8))]
pub struct TokenPair {
    // Do not reorder the fields here. The ordering is explicitly used by repr(C)
    // to make this struct equivalent to a u64.
    #[cfg(target_endian = "little")]
    pub left: Token,
    pub right: Token,
    #[cfg(target_endian = "big")]
    pub left: Token,
}

// By not short-circuiting in comparisons, we get better codegen.
// See <https://github.com/rust-lang/rust/issues/117800>
impl PartialEq for TokenPair {
    #[inline]
    fn eq(&self, other: &TokenPair) -> bool {
        // By using `to_bits`, the codegen can be optimized out even
        // further potentially. Relies on the correct alignment/field
        // order of `TokenPair`.
        self.to_bits() == other.to_bits()
    }
}

impl Eq for TokenPair {}

impl core::hash::Hash for TokenPair {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.to_bits().hash(state);
    }
}

impl TokenPair {
    #[inline]
    pub fn new<T: Into<Token>>(left: T, right: T) -> Self {
        Self {
            left: left.into(),
            right: right.into(),
        }
    }

    #[inline(always)]
    const fn to_bits(self) -> u64 {
        self.left.to_bits() as u64 | ((self.right.to_bits() as u64) << 32)
    }
}

impl AsRef<TokenPair> for TokenPair {
    fn as_ref(&self) -> &TokenPair {
        self
    }
}
