//! Crate for defining and handling `nailpit` configuration. Defines the main
//! [`NailConfig`] struct, as well as the utility method to derive the config object
//! from either `toml` files or environment variables, with the format
//! `PIT__GENERATOR__TIMEOUT` as an example.

use std::ops::Deref;

use color_eyre::Result;
use serde_aux::field_attributes::{
    deserialize_bool_from_anything, deserialize_number_from_string,
    deserialize_option_number_from_string,
};

#[derive(Debug, serde::Deserialize)]
pub struct NailConfig {
    pub pit_routes: Vec<String>,
    pub socket_addr: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub worker_threads: usize,
    pub generator: GeneratorConfig,
    #[serde(default)]
    pub rate_limiting: RateLimitingConfig,
    pub open_telemetry: OpenTelemetryConfig,
}

#[derive(Default, serde::Deserialize)]
pub struct PromptsList(Vec<String>);

impl std::fmt::Debug for PromptsList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PromptsList").finish_non_exhaustive()
    }
}

impl Deref for PromptsList {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct GeneratorConfig {
    #[serde(default)]
    pub prompts: PromptsList,
    pub input_files: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub min_paragraph_size: usize,
    #[serde(deserialize_with = "deserialize_option_number_from_string")]
    pub max_paragraph_size: Option<usize>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub payload_size: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub timeout: u64,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub min_delay: u64,
    #[serde(deserialize_with = "deserialize_option_number_from_string")]
    pub max_delay: Option<u64>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub chunk_size: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub header_size: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub max_pit_links: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "mode")]
pub enum DropBehavior {
    #[default]
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "spicy")]
    Spicy { payload: Vec<String> },
}

impl DropBehavior {
    pub fn is_spicy(&self) -> bool {
        matches!(self, Self::Spicy { .. })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "type")]
pub enum RateLimitingConfig {
    #[default]
    #[serde(rename = "no_limit")]
    NoLimit,
    #[serde(rename = "soft_limit")]
    SoftLimit {
        #[serde(deserialize_with = "deserialize_number_from_string")]
        soft_limit: u64,
        #[serde(deserialize_with = "deserialize_number_from_string")]
        soft_delay: u64,
    },
    #[serde(rename = "hard_limit")]
    HardLimit {
        #[serde(deserialize_with = "deserialize_number_from_string")]
        hard_limit: u64,
        #[serde(default)]
        drop_behavior: DropBehavior,
    },
    #[serde(rename = "soft_with_hard_limit")]
    SoftWithHardLimit {
        #[serde(deserialize_with = "deserialize_number_from_string")]
        soft_limit: u64,
        #[serde(deserialize_with = "deserialize_number_from_string")]
        hard_limit: u64,
        #[serde(deserialize_with = "deserialize_number_from_string")]
        soft_delay: u64,
        #[serde(default)]
        drop_behavior: DropBehavior,
    },
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenTelemetryConfig {
    pub endpoint: String,
    pub service_name: String,
    #[serde(deserialize_with = "deserialize_bool_from_anything")]
    pub logs: bool,
    #[serde(deserialize_with = "deserialize_bool_from_anything")]
    pub traces: bool,
}

pub fn get_configuration() -> Result<NailConfig> {
    let config_dir = std::env::current_dir()?.join("configuration");

    let config = config::Config::builder()
        .add_source(
            config::File::from(config_dir.join("pit.default.toml"))
                .format(config::FileFormat::Toml),
        )
        .add_source(
            config::File::from(config_dir.join("pit.toml"))
                .required(false)
                .format(config::FileFormat::Toml),
        )
        .add_source(
            config::Environment::with_prefix("PIT")
                .prefix_separator("__")
                .separator("__"),
        )
        .build()?;

    Ok(config.try_deserialize()?)
}
