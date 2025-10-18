//! [`TokenWeights`] are representations of how common [`Token`]s are, and are paired up with
//! a [`TokenPair`](crate::token::TokenPair) in a [`NailKov`](crate::NailKov).

use indexmap::IndexMap;
use rand::Rng;
use rand_distr::{Distribution, weighted::WeightedAliasIndex};

use crate::{RandomState, error::NailError, token::Token};

/// A distribution of choices and their likelihood.
#[derive(Clone, Debug)]
pub struct TokenWeights {
    /// Mappings of choice indexes to their likelihood.
    dist: WeightedAliasIndex<u64>,
    /// The actual choices
    choices: Box<[Token]>,
}

impl Distribution<Token> for TokenWeights {
    #[inline]
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Token {
        // SAFETY: The sampled index from `dist` will always correspond to a valid
        // token in the `choices` slice.
        unsafe { *self.choices.get_unchecked(self.dist.sample(rng)) }
    }
}

/// Builder for [`TokenWeights`].
#[derive(Clone, Debug)]
pub struct TokenWeightsBuilder {
    /// Counts how many times a token is likely to appear.
    occurrences: IndexMap<Token, u64, RandomState>,
}

impl TokenWeightsBuilder {
    pub fn new(hasher: RandomState) -> Self {
        Self {
            occurrences: IndexMap::with_hasher(hasher),
        }
    }

    /// Creates a weighted distribution for the likelihood of tokens to appear.
    pub fn build(self) -> Result<TokenWeights, NailError> {
        let (choices, counts): (Vec<_>, Vec<_>) = self.occurrences.into_iter().unzip();

        if choices.is_empty() {
            return Err(NailError::EmptyInput);
        }

        Ok(TokenWeights {
            dist: WeightedAliasIndex::new(counts)?,
            choices: choices.into(),
        })
    }

    /// Count an occurrence of this token, or add it if it hasn't been seen before.
    pub fn add(&mut self, token: Token) {
        self.occurrences
            .entry(token)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }
}

impl Default for TokenWeightsBuilder {
    fn default() -> Self {
        Self::new(RandomState::new())
    }
}
