use std::sync::Arc;

use nailgen::Template;
use nailkov::interner::Interner;

#[derive(Clone)]
pub struct App {
    pub config: Arc<nailconfig::NailConfig>,
    pub inputs: Arc<[nailgen::MarkovGen]>,
    pub interner: Arc<Interner>,
    pub spicy: Option<Arc<nailspicy::SpicyPayloads>>,
    pub templates: Arc<[Template]>,
}

impl App {
    pub fn new(
        config: Arc<nailconfig::NailConfig>,
        inputs: Arc<[nailgen::MarkovGen]>,
        interner: Arc<Interner>,
        spicy: Option<Arc<nailspicy::SpicyPayloads>>,
        templates: Arc<[Template]>,
    ) -> Self {
        Self {
            config,
            inputs,
            interner,
            spicy,
            templates,
        }
    }
}
