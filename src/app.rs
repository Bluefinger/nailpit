use std::sync::Arc;

#[derive(Clone)]
pub struct App {
    pub config: Arc<nailconfig::NailConfig>,
    pub inputs: Arc<[nailgen::MarkovGen]>,
    pub spicy: Option<Arc<nailspicy::SpicyPayloads>>,
}

impl App {
    pub fn new(
        config: Arc<nailconfig::NailConfig>,
        inputs: Arc<[nailgen::MarkovGen]>,
        spicy: Option<Arc<nailspicy::SpicyPayloads>>,
    ) -> Self {
        Self {
            config,
            inputs,
            spicy,
        }
    }
}
