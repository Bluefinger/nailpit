use std::{convert::Infallible, ops::Deref, sync::Arc};

use axum::extract::{FromRef, FromRequestParts};
use nailconfig::NailConfig;
use nailgen::MarkovGen;
use nailkov::interner::Interner;
use nailrng::FastRng;
use rand::seq::IndexedRandom;

/// Smart pointer for all available Markov chains.
#[derive(Clone)]
pub struct NailInputs {
    chains: Arc<[MarkovGen]>,
    interner: Arc<Interner>,
}

impl NailInputs {
    pub fn new(chains: Arc<[MarkovGen]>, interner: Arc<Interner>) -> Self {
        Self { chains, interner }
    }

    /// Pulls a random markov chain from the available list. Returns a cloned
    /// pointer to the selected chain.
    pub fn get_random_input(&self) -> MarkovGen {
        assert!(!self.chains.is_empty());

        if self.chains.len() == 1 {
            self.chains[0].clone()
        } else {
            let mut rng = FastRng::default();

            self.chains.choose(&mut rng).unwrap().clone()
        }
    }

    pub fn get_interner(&self) -> Arc<Interner> {
        self.interner.clone()
    }
}

impl std::fmt::Debug for NailInputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NailInputs").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig(Arc<NailConfig>);

impl AppConfig {
    pub fn clone_inner(&self) -> Arc<NailConfig> {
        self.0.clone()
    }
}

impl From<Arc<NailConfig>> for AppConfig {
    #[inline]
    fn from(value: Arc<NailConfig>) -> Self {
        Self(value)
    }
}

impl Deref for AppConfig {
    type Target = NailConfig;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct ServerState {
    pub config: AppConfig,
    pub inputs: NailInputs,
}

impl ServerState {
    pub fn new(
        config: impl Into<AppConfig>,
        chains: Arc<[MarkovGen]>,
        interner: Arc<Interner>,
    ) -> Self {
        let config = config.into();

        Self {
            config,
            inputs: NailInputs { chains, interner },
        }
    }
}

impl FromRef<ServerState> for AppConfig {
    #[inline]
    fn from_ref(input: &ServerState) -> Self {
        input.config.clone()
    }
}

impl FromRef<ServerState> for NailInputs {
    #[inline]
    fn from_ref(input: &ServerState) -> Self {
        input.inputs.clone()
    }
}

impl<S> FromRequestParts<S> for AppConfig
where
    AppConfig: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    #[fastrace::trace]
    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(AppConfig::from_ref(state))
    }
}

impl<S> FromRequestParts<S> for NailInputs
where
    NailInputs: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    #[fastrace::trace]
    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(NailInputs::from_ref(state))
    }
}
