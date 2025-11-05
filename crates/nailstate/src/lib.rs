use std::{ops::Deref, sync::Arc};

use nailconfig::NailConfig;
use nailgen::{MarkovGen, Template};
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
         match self.chains.as_ref() {
            [] => {
                panic!("There must be at least one markov chain");
            }
            [chain] => chain.clone(),
            chains => chains.choose(rng).unwrap().clone(),
        }
    }

    #[inline]
    pub fn get_interner(&self) -> Arc<Interner> {
        self.interner.clone()
    }

    pub fn get_warning_template(&self) -> Template {
        self.templates
            .iter()
            .find_map(|template| {
                if let template @ Template::Warning(_) = template {
                    Some(template.clone())
                } else {
                    None
                }
            })
            .expect("There must be a Warning template")
    }

    pub fn get_generated_template(&self) -> Template {
        self.templates
            .iter()
            .find_map(|template| {
                if let template @ Template::Generated(_) = template {
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
