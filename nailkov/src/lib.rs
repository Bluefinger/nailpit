mod token;
mod distribution;

use hashbrown::HashMap;

use distribution::TokenDistribution;
use token::TokenPair;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct NailChain<S> {
    map: HashMap<TokenPair, TokenDistribution, S>,
}
