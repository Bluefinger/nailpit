//! Crate defining a Markov Chain implementation, and a string interner for use
//! with the markov chain.

mod distribution;
mod error;
pub mod interner;
mod token;

use error::NailError;
use indexmap::IndexMap;
use interner::Interner;
use itertools::Itertools;
use nailrng::FastRng;
use rand::{RngCore, seq::IteratorRandom};
use rand_distr::Distribution;

use distribution::{TokenWeights, TokenWeightsBuilder};
use rustc_hash::FxHasher;
use token::{Token, TokenPair};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub struct RandomState {
    seed: usize,
}

impl RandomState {
    fn new() -> Self {
        let mut rng = FastRng::default();

        Self {
            seed: rng.next_u64() as usize,
        }
    }
}

impl Default for RandomState {
    fn default() -> Self {
        Self::new()
    }
}

impl core::hash::BuildHasher for RandomState {
    type Hasher = FxHasher;

    fn build_hasher(&self) -> Self::Hasher {
        FxHasher::with_seed(self.seed)
    }
}

#[derive(Clone, Debug)]
pub struct NailKov {
    chain: IndexMap<TokenPair, TokenWeights, RandomState>,
}

pub struct NailKovIter<'a, R: RngCore> {
    rng: &'a mut R,
    markov: &'a NailKov,
    prev: TokenPair,
}

impl<R: RngCore> Iterator for NailKovIter<'_, R> {
    type Item = Token;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let dist = self.markov.chain.get(&self.prev)?;

        let next_token = dist.sample(&mut self.rng);

        self.prev = TokenPair::new(self.prev.right, next_token);

        Some(next_token)
    }
}

impl NailKov {
    #[inline]
    pub fn generate_tokens<'a, R: RngCore>(&'a self, rng: &'a mut R) -> NailKovIter<'a, R> {
        NailKovIter {
            // A markov chain that was successfully built is never empty, so
            // it will always return with a value, making unwrapping it safe to do.
            prev: self.chain.keys().choose(rng).copied().unwrap(),
            markov: self,
            rng,
        }
    }
}

impl NailKov {
    pub fn from_input(interner: &mut Interner, input: &str) -> Result<NailKov, NailError> {
        NailBuilder::new(RandomState::new()).with_input(interner, input)
    }
}

struct NailBuilder {
    chain: IndexMap<TokenPair, TokenWeightsBuilder, RandomState>,
}

impl NailBuilder {
    fn new(hasher: RandomState) -> Self {
        Self {
            chain: IndexMap::with_hasher(hasher),
        }
    }

    fn with_input(self, interned: &mut Interner, input: &str) -> Result<NailKov, NailError> {
        self.feed_str(interned, input)?.build()
    }

    fn build(self) -> Result<NailKov, NailError> {
        if self.chain.is_empty() {
            return Err(NailError::EmptyInput);
        }

        let chain: IndexMap<TokenPair, TokenWeights, RandomState> = self
            .chain
            .into_iter()
            .flat_map(|(pair, dist)| {
                dist.build()
                    .inspect_err(|err| log::error!("Weight error {pair:?}: {err}"))
                    .map(|build| (pair, build))
            })
            .collect();

        if chain.is_empty() {
            return Err(NailError::EmptyInput);
        }

        Ok(NailKov { chain })
    }

    /// Add the occurrence of `next` following `prev`.
    fn add_token_pair(&mut self, prev: TokenPair, next: Token) {
        match self.chain.get_mut(&prev) {
            Some(builder) => {
                builder.add(next);
            }
            None => {
                let mut builder = TokenWeightsBuilder::new(self.chain.hasher().clone());
                builder.add(next);
                self.chain.insert(prev, builder);
            }
        }
    }

    fn feed_str(self, interner: &mut Interner, content: &str) -> Result<Self, NailError> {
        self.feed_tokens(
            content
                .split_word_bounds()
                .map(|text| interner.intern(text)),
        )
    }

    fn feed_tokens(mut self, tokens: impl Iterator<Item = Token>) -> Result<Self, NailError> {
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
