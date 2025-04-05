//! [`TokenWeights`] are representations of how common [`Token`]s are, and are paired up with
//! a [`TokenPair`](crate::token::TokenPair) in a [`NailKov`](crate::NailKov).

use core::hash::BuildHasher;
use hashbrown::HashMap;
use rand::Rng;
use rand_distr::{Distribution, weighted::WeightedAliasIndex};
use wyrand::RandomWyHashState;

use crate::token::Token;

/// A distribution of choices and their likelihood.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TokenWeights {
    /// Mappings of choice indexes to their likelihood.
    dist: WeightedAliasIndex<u64>,
    /// The actual choices
    choices: Box<[Token]>,
}

impl TokenWeights {
    pub fn sample_token(&self, rng: &mut impl Rng) -> &str {
        // SAFETY: The sampled indices will always be in sync with with the tokens
        // vector, so it will never go out of bounds. We will also never have an empty
        // TokenWeights instance.
        unsafe { self.choices.get_unchecked(self.dist.sample(rng)) }
    }
}

/// Builder for [`TokenWeights`].
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TokenWeightsBuilder<S = RandomWyHashState> {
    /// Counts how many times a token is likely to appear.
    occurrences: HashMap<Token, u64, S>,
}

impl<S: BuildHasher> TokenWeightsBuilder<S> {
    pub fn new(hasher: S) -> Self {
        Self {
            occurrences: HashMap::with_hasher(hasher),
        }
    }

    /// Creates a weighted distribution for the likelihood of tokens to appear.
    pub fn build(self) -> Option<TokenWeights> {
        let (choices, counts): (Vec<_>, Vec<_>) = self.occurrences.into_iter().unzip();

        if choices.is_empty() {
            return None;
        }

        Some(TokenWeights {
            dist: WeightedAliasIndex::new(counts)
                .inspect_err(|err| log::error!("Error calculating weights: {}", err))
                .ok()?,
            choices: choices.into(),
        })
    }

    /// Count an occurrence of this token, or add it if it hasn't been seen before.
    pub fn add(&mut self, token: &str) {
        match self.occurrences.get_mut(token) {
            Some(n) => {
                *n += 1;
            }
            None => {
                self.occurrences.insert(token.into(), 1);
            }
        }
    }
}

impl Default for TokenWeightsBuilder<RandomWyHashState> {
    fn default() -> Self {
        Self::new(RandomWyHashState::new())
    }
}
