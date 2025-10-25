use std::{convert::Infallible, ops::Deref, sync::Arc};

use axum::extract::{FromRef, FromRequestParts};
use nailconfig::NailConfig;
use nailgen::{GeneratedTemplate, MarkovGen, Template, WarningTemplate};
use nailkov::interner::Interner;
use nailrng::FastRng;
use rand::seq::IndexedRandom;

/// Smart pointer for all available Markov chains.
#[derive(Clone)]
pub struct NailInputs {
    chains: Arc<[MarkovGen]>,
    interner: Arc<Interner>,
    templates: Arc<[Template]>,
}

impl NailInputs {
    pub fn new(
        chains: Arc<[MarkovGen]>,
        interner: Arc<Interner>,
        templates: Arc<[Template]>,
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

    pub fn get_warning_template(&self) -> WarningTemplate {
        self.templates
            .iter()
            .find_map(|template| {
                if let Template::Warning(template) = template {
                    Some(template.clone())
                } else {
                    None
                }
            })
            .expect("There must be a Warning template")
    }

    pub fn get_generated_template(&self) -> GeneratedTemplate {
        self.templates
            .iter()
            .find_map(|template| {
                if let Template::Generated(template) = template {
                    Some(template.clone())
                } else {
                    None
                }
            })
            .expect("There must be a Generated template")
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
        templates: Arc<[Template]>,
    ) -> Self {
        let config = config.into();

        Self {
            config,
            inputs: NailInputs {
                chains,
                interner,
                templates,
            },
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

impl FromRef<ServerState> for WarningTemplate {
    #[inline]
    fn from_ref(input: &ServerState) -> Self {
        input.inputs.get_warning_template()
    }
}

impl FromRef<ServerState> for GeneratedTemplate {
    #[inline]
    fn from_ref(input: &ServerState) -> Self {
        input.inputs.get_generated_template()
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

impl FromRequestParts<ServerState> for WarningTemplate
where
    WarningTemplate: FromRef<ServerState>,
    ServerState: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        Ok(WarningTemplate::from_ref(state))
    }
}

impl FromRequestParts<ServerState> for GeneratedTemplate
where
    GeneratedTemplate: FromRef<ServerState>,
    ServerState: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        Ok(GeneratedTemplate::from_ref(state))
    }
}
