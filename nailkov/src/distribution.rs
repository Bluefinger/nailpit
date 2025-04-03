//! [`TokenDistribution`] are representations of how common [`Token`]s are, and are paired up with
//! a [`TokenPair`](crate::token::TokenPair) in a [`Chain`](crate::Chain).

use rand::{Rng, seq::WeightError};
use rand_distr::{Distribution, weighted::WeightedAliasIndex};
use core::hash::BuildHasher;
use hashbrown::HashMap;
use wyrand::RandomWyHashState;

use crate::token::Token;

/// A distribution of choices and their likelyhood.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TokenDistribution {
    /// Mappings of index in choices to their likelyhood.
    dist: WeightedAliasIndex<u64>,
    /// The actual choices
    choices: Vec<Token>,
}

impl TokenDistribution {
    pub fn sample_token(&self, rng: &mut impl Rng) -> Option<&Token> {
        self.choices.get(self.dist.sample(rng))
    }
}

/// Builder for [`TokenDistribution`]. Used when parsing a text to add a lot of words, and then to
/// build a list of [`TokenDistribution`] using how many times they appeared.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TokenDistributionBuilder<S = RandomWyHashState> {
    /// Counts how many times a token is likely to appear.
    map: HashMap<String, u64, S>,
}

impl<S: BuildHasher> TokenDistributionBuilder<S> {
    pub fn new(hasher: S) -> Self {
        Self {
            map: HashMap::with_hasher(hasher),
        }
    }

    /// Creates a weighted distribution for the likelyhood of tokens to appear.
    pub fn build(self) -> Result<TokenDistribution, WeightError> {
        let (choices, counts): (Vec<_>, Vec<_>) = self.map.into_iter().unzip();

        Ok(TokenDistribution {
            dist: WeightedAliasIndex::new(counts)?,
            choices,
        })
    }

    /// Count an occurance of this token, or add it if it hasn't been seen before.
    pub fn add(&mut self, token: &str) {
        match self.map.get_mut(token) {
            Some(n) => {
                *n += 1;
            }
            None => {
                self.map.insert(token.to_string(), 1);
            }
        }
    }
}

impl Default for TokenDistributionBuilder<RandomWyHashState> {
    fn default() -> Self {
        Self::new(RandomWyHashState::new())
    }
}
