//! Crate defining a Markov Chain implementation, and a string interner for use
//! with the markov chain.

mod distribution;
mod error;
pub mod interner;
mod token;

use error::NailError;
use hashbrown::HashMap;
use indexmap::IndexMap;
use interner::Interner;
use itertools::Itertools;
use nailrng::FastRng;
use rand::{seq::IteratorRandom, RngCore};
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
    chain: HashMap<TokenPair, TokenWeights, RandomState>,
}

pub struct NailKovIter<'a, R: RngCore> {
    rng: &'a mut R,
    chain: &'a NailKov,
    prev: TokenPair,
}

impl<R: RngCore> Iterator for NailKovIter<'_, R> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let next_token = self.chain.generate_next_token(&mut self.rng, self.prev)?;

        self.prev = TokenPair::new(self.prev.right, next_token);

        Some(next_token)
    }
}

impl NailKov {
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

        let chain: HashMap<TokenPair, TokenWeights, RandomState> = self
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
