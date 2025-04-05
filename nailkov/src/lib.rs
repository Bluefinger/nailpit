mod distribution;
mod error;
mod token;

use error::NailError;
use hashbrown::HashMap;
use itertools::Itertools;
use rand::{RngCore, seq::IteratorRandom};
use std::hash::BuildHasher;

use distribution::{TokenWeights, TokenWeightsBuilder};
use token::{TokenPair, TokenPairRef, TokenRef};
use unicode_segmentation::UnicodeSegmentation;
use wyrand::RandomWyHashState;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct NailKov<S = RandomWyHashState> {
    chain: HashMap<TokenPair, TokenWeights, S>,
}

pub struct NailKovIter<'a, R: RngCore, S = RandomWyHashState> {
    rng: &'a mut R,
    chain: &'a NailKov<S>,
    prev: TokenPairRef<'a>,
}

impl<'a, R: RngCore, S: BuildHasher> Iterator for NailKovIter<'a, R, S> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let next_token = self.chain.generate_next_token(&mut self.rng, &self.prev)?;

        self.prev = (self.prev.1, next_token);

        Some(next_token)
    }
}

impl<S: BuildHasher> NailKov<S> {
    fn generate_next_token(
        &self,
        rng: &mut impl RngCore,
        prev: &TokenPairRef<'_>,
    ) -> Option<TokenRef<'_>> {
        self.chain.get(prev).map(|dist| dist.sample_token(rng))
    }

    pub fn generate_tokens<'a, R: RngCore>(
        &'a self,
        rng: &'a mut R,
    ) -> impl Iterator<Item = &'a str> {
        self.starting_token_pair(rng)
            .map(TokenPair::as_token_pair_ref)
            .map(|prev| NailKovIter {
                prev,
                rng,
                chain: self,
            })
            .into_iter()
            .flatten()
    }

    fn pairs(&self) -> impl Iterator<Item = &TokenPair> {
        self.chain.keys()
    }

    fn starting_token_pair(&self, rng: &mut impl RngCore) -> Option<&TokenPair> {
        self.pairs().choose(rng)
    }
}

impl core::str::FromStr for NailKov<RandomWyHashState> {
    type Err = NailError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        NailBuilder::default().with_input(input)
    }
}

impl<S: BuildHasher + Clone + Default> NailKov<S> {
    pub fn from_str_with_hasher(input: &str, hasher: S) -> Result<NailKov<S>, NailError> {
        NailBuilder::new(hasher).with_input(input)
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NailBuilder<S = RandomWyHashState> {
    chain: HashMap<TokenPair, TokenWeightsBuilder<S>, S>,
}

impl<S: BuildHasher + Clone + Default> NailBuilder<S> {
    pub fn new(hasher: S) -> Self {
        Self {
            chain: HashMap::with_hasher(hasher),
        }
    }

    pub fn with_input(self, input: &str) -> Result<NailKov<S>, NailError> {
        self.feed_str(input)?.build()
    }

    pub fn build(self) -> Result<NailKov<S>, NailError> {
        if self.chain.is_empty() {
            return Err(NailError::EmptyInput);
        }

        let chain: HashMap<TokenPair, TokenWeights, S> = self
            .chain
            .into_iter()
            .flat_map(|(pair, dist)| {
                dist.build()
                    .inspect_err(|err| log::error!("Weight error {:?}: {}", pair, err))
                    .map(|build| (pair, build))
            })
            .collect();

        if chain.is_empty() {
            return Err(NailError::BuildError);
        }

        Ok(NailKov { chain })
    }

    /// Add the occurrence of `next` following `prev`.
    fn add_token_pair(&mut self, prev: TokenPairRef<'_>, next: &str) {
        match self.chain.get_mut(&prev) {
            Some(builder) => {
                builder.add(next);
            }
            None => {
                let mut builder = TokenWeightsBuilder::new(self.chain.hasher().clone());
                builder.add(next);
                self.chain.insert(TokenPair::from(&prev), builder);
            }
        }
    }

    pub fn feed_str(self, content: &str) -> Result<Self, NailError> {
        self.feed_tokens(content.split_word_bounds())
    }

    fn feed_tokens<'token, T: Iterator<Item = TokenRef<'token>>>(
        mut self,
        tokens: T,
    ) -> Result<Self, NailError> {
        let windows = tokens.tuple_windows();

        if windows.size_hint().1.is_none() {
            return Err(NailError::EmptyInput);
        }

        for (left, right, next) in windows {
            self.add_token_pair((left, right), next);
        }

        Ok(self)
    }
}

impl Default for NailBuilder<RandomWyHashState> {
    fn default() -> Self {
        Self::new(RandomWyHashState::new())
    }
}
