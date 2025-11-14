use std::{convert::Infallible, ops::Deref, sync::Arc};

use axum::extract::{FromRef, FromRequestParts};
use nailconfig::NailConfig;
use nailgen::{GeneratedTemplate, MarkovGen, Template, WarningTemplate};
use nailkov::interner::Interner;
use nailrng::FastRng;
use nailspicy::SpicyPayloads;
use rand::seq::IndexedRandom;

pub struct Templates {
    pub warning: WarningTemplate,
    pub generated: GeneratedTemplate,
}

#[derive(Clone)]
pub struct NailPayloads {
    spicy_payloads: Option<Arc<SpicyPayloads>>,
}

impl core::fmt::Debug for NailPayloads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NailPayloads").finish_non_exhaustive()
    }
}

impl NailPayloads {
    pub fn get(&self) -> Option<Arc<SpicyPayloads>> {
        self.spicy_payloads.clone()
    }
}

/// Smart pointer for all available Markov chains.
#[derive(Clone)]
pub struct NailInputs {
    chains: Arc<[MarkovGen]>,
    interner: Arc<Interner>,
    templates: Arc<Templates>,
}

impl NailInputs {
    pub fn new(
        chains: Arc<[MarkovGen]>,
        interner: Arc<Interner>,
        templates: Arc<Templates>,
    ) -> Self {
        Self {
            chains,
            interner,
            templates,
        }
    }

    /// Pulls a random markov chain from the available list. Returns a cloned
    /// pointer to the selected chain.
    #[inline]
    pub fn get_random_input(&self, rng: &mut FastRng) -> MarkovGen {
        assert!(!self.chains.is_empty());

        if self.chains.len() == 1 {
            self.chains[0].clone()
        } else {
            self.chains.choose(rng).unwrap().clone()
        }
    }

    #[inline]
    pub fn get_interner(&self) -> Arc<Interner> {
        self.interner.clone()
    }

    #[inline]
    pub fn get_warning_template(&self) -> Template {
        Template::from(self.templates.warning.clone())
    }

    #[inline]
    pub fn get_generated_template(&self) -> Template {
        Template::from(self.templates.generated.clone())
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
    pub spicy_payloads: NailPayloads,
}

impl ServerState {
    pub fn new(
        config: impl Into<AppConfig>,
        chains: Arc<[MarkovGen]>,
        interner: Arc<Interner>,
        templates: Arc<Templates>,
        spicy_payloads: Option<Arc<SpicyPayloads>>,
    ) -> Self {
        let config = config.into();

        Self {
            config,
            inputs: NailInputs {
                chains,
                interner,
                templates,
            },
            spicy_payloads: NailPayloads { spicy_payloads },
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

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(NailInputs::from_ref(state))
    }
}
