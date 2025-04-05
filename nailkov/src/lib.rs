mod distribution;
mod token;

use hashbrown::HashMap;
use itertools::Itertools;
use rand::{RngCore, seq::IteratorRandom};
use std::hash::BuildHasher;

use distribution::{TokenDistribution, TokenDistributionBuilder};
use token::{TokenPair, TokenPairRef, TokenRef};
use unicode_segmentation::UnicodeSegmentation;
use wyrand::RandomWyHashState;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct NailKov<S = RandomWyHashState> {
    chain: HashMap<TokenPair, TokenDistribution, S>,
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
        self.chain
            .get(prev)
            .map(|dist| dist.sample_token(rng))
            .map(String::as_str)
    }

    pub fn generate_tokens<'a, R: RngCore>(
        &'a self,
        rng: &'a mut R,
    ) -> impl Iterator<Item = &'a str> {
        self.starting_token_pair(rng)
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

    fn starting_token_pair(&self, rng: &mut impl RngCore) -> Option<TokenPairRef<'_>> {
        self.pairs().choose(rng).map(|pair| pair.as_ref())
    }
}

impl NailKov<RandomWyHashState> {
    pub fn from_str(input: &str) -> Option<Self> {
        NailBuilder::default()
            .feed_str(input)
            .and_then(|nail| nail.build())
    }
}

impl<S: BuildHasher + Clone + Default> NailKov<S> {
    pub fn from_str_with_hasher(input: &str, hasher: S) -> Option<Self> {
        NailBuilder::new(hasher)
            .feed_str(input)
            .and_then(|nail| nail.build())
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NailBuilder<S = RandomWyHashState> {
    map: HashMap<TokenPair, TokenDistributionBuilder<S>, S>,
}

impl<S: BuildHasher + Clone + Default> NailBuilder<S> {
    pub fn new(hasher: S) -> Self {
        Self {
            map: HashMap::with_hasher(hasher),
        }
    }

    pub fn build(self) -> Option<NailKov<S>> {
        if self.map.is_empty() {
            return None;
        }

        let chain_map = self
            .map
            .into_iter()
            .flat_map(|(pair, dist)| dist.build().map(|build| (pair, build)))
            .collect();

        Some(NailKov { chain: chain_map })
    }

    /// Add the occurrence of `next` following `prev`.
    pub fn add_token_pair(&mut self, prev: TokenPairRef<'_>, next: &str) {
        match self.map.get_mut(&prev) {
            Some(b) => {
                b.add(next);
            }
            None => {
                let mut b = TokenDistributionBuilder::new(self.map.hasher().clone());
                b.add(next);
                let tp = TokenPair::from(&prev);
                self.map.insert(tp, b);
            }
        }
    }

    pub fn feed_str(self, content: &str) -> Option<Self> {
        self.feed_tokens(content.split_word_bounds())
    }

    fn feed_tokens<'a, T: Iterator<Item = TokenRef<'a>>>(mut self, tokens: T) -> Option<Self> {
        let windows = tokens.tuple_windows();

        if windows.size_hint().1.is_none() {
            return None;
        }

        for (left, right, next) in windows {
            self.add_token_pair((left, right), next);
        }

        Some(self)
    }
}

impl Default for NailBuilder<RandomWyHashState> {
    fn default() -> Self {
        Self::new(RandomWyHashState::new())
    }
}
