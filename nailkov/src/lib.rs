#![warn(clippy::undocumented_unsafe_blocks)]

mod distribution;
mod error;
pub mod interner;
mod token;

use error::NailError;
use hashbrown::HashMap;
use interner::Interner;
use itertools::Itertools;
use rand::{RngCore, seq::IteratorRandom};
use rand_distr::Distribution;
use std::hash::BuildHasher;

use distribution::{TokenWeights, TokenWeightsBuilder};
use token::{Token, TokenPair};
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
    prev: TokenPair,
}

impl<R: RngCore, S: BuildHasher> Iterator for NailKovIter<'_, R, S> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let next_token = self.chain.generate_next_token(&mut self.rng, self.prev)?;

        self.prev = TokenPair::new(self.prev.1, next_token);

        Some(next_token)
    }
}

impl<S: BuildHasher> NailKov<S> {
    fn generate_next_token(&self, rng: &mut impl RngCore, prev: TokenPair) -> Option<Token> {
        self.chain.get(&prev).map(|dist| dist.sample(rng))
    }

    pub fn generate_tokens<'a, R: RngCore>(
        &'a self,
        rng: &'a mut R,
    ) -> impl Iterator<Item = Token> {
        self.starting_token_pair(rng)
            .map(|&prev| NailKovIter {
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

impl NailKov<RandomWyHashState> {
    pub fn from_input(interner: &mut Interner, input: &str) -> Result<NailKov, NailError> {
        NailBuilder::new(RandomWyHashState::new()).with_input(interner, input)
    }
}

impl<S: BuildHasher + Clone + Default> NailKov<S> {
    pub fn from_input_with_hasher(
        interner: &mut Interner,
        input: &str,
        hasher: S,
    ) -> Result<NailKov<S>, NailError> {
        NailBuilder::new(hasher).with_input(interner, input)
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct NailBuilder<S = RandomWyHashState> {
    chain: HashMap<TokenPair, TokenWeightsBuilder<S>, S>,
}

impl<S: BuildHasher + Clone + Default> NailBuilder<S> {
    fn new(hasher: S) -> Self {
        Self {
            chain: HashMap::with_hasher(hasher),
        }
    }

    fn with_input(self, interned: &mut Interner, input: &str) -> Result<NailKov<S>, NailError> {
        self.feed_str(interned, input)?.build()
    }

    fn build(self) -> Result<NailKov<S>, NailError> {
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
    fn add_token_pair(&mut self, prev: TokenPair, next: impl Into<Token>) {
        match self.chain.get_mut(&prev) {
            Some(builder) => {
                builder.add(next.into());
            }
            None => {
                let mut builder = TokenWeightsBuilder::new(self.chain.hasher().clone());
                builder.add(next.into());
                self.chain.insert(prev, builder);
            }
        }
    }

    fn feed_str(self, interner: &mut Interner, content: &str) -> Result<Self, NailError> {
        self.feed_tokens(
            content
                .split_word_bounds()
                .map(|text| interner.intern(text).into()),
        )
    }

    fn feed_tokens<T: Iterator<Item = Token>>(mut self, tokens: T) -> Result<Self, NailError> {
        let windows = tokens.tuple_windows();

        if windows.size_hint().1.is_none() {
            return Err(NailError::EmptyInput);
        }

        for (left, right, next) in windows {
            self.add_token_pair(TokenPair::new(left, right), next);
        }

        Ok(self)
    }
}
