use std::ops::Deref;

/// Representation of a string segment.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C, align(4))]
pub struct Token(u32);

impl Token {
    #[inline(always)]
    pub const fn new(ptr: u32) -> Self {
        Self(ptr)
    }

    #[inline(always)]
    pub(crate) const fn index(&self) -> usize {
        self.0 as usize
    }

    #[inline(always)]
    const fn to_bits(self) -> u32 {
        self.0
    }
}

impl Deref for Token {
    type Target = u32;

    #[inline]
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
    #[inline(always)]
    fn eq(&self, other: &TokenPair) -> bool {
        // By using `to_bits`, the codegen can be optimized out even
        // further potentially. Relies on the correct alignment/field
        // order of `TokenPair`.
        self.to_bits() == other.to_bits()
    }
}

impl Eq for TokenPair {}

impl core::hash::Hash for TokenPair {
    #[inline(always)]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.to_bits().hash(state);
    }
}

impl TokenPair {
    #[inline(always)]
    pub const fn new(left: Token, right: Token) -> Self {
        Self { left, right }
    }

    #[inline(always)]
    const fn to_bits(self) -> u64 {
        self.left.to_bits() as u64 | ((self.right.to_bits() as u64) << 32)
    }
}

impl AsRef<TokenPair> for TokenPair {
    #[inline]
    fn as_ref(&self) -> &TokenPair {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_smoke_testing() {
        let left = Token(0x2);
        let right = Token(0x2b);

        let pair = TokenPair::new(left, right);

        assert_eq!(pair.to_bits(), 0x2b00000002);
        assert_eq!(pair.left, left);
        assert_eq!(pair.right, right);

        let other_right = Token(0x2c);

        let other_pair = TokenPair::new(left, other_right);

        assert_eq!(other_pair.to_bits(), 0x2c00000002);
        assert_ne!(pair, other_pair);
    }
}
