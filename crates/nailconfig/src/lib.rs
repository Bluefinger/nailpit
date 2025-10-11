//! Crate for defining and handling `nailpit` configuration. Defines the main
//! [`NailConfig`] struct, as well as the utility method to derive the config object
//! from various `toml` files.

use core::num::NonZero;
use std::ops::Deref;

use color_eyre::Result;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NailConfig {
    pub server: ServerConfig,
    pub generator: GeneratorConfig,
    #[serde(default)]
    pub rate_limiting: RateLimitingConfig,
    pub open_telemetry: OpenTelemetryConfig,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerConfig {
    pub pit_routes: Vec<String>,
    pub socket_addr: String,
    pub worker_threads: NonZero<usize>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GeneratorConfig {
    #[serde(default)]
    pub prompts: PromptsList,
    pub input_files: String,
    pub min_paragraph_size: usize,
    pub max_paragraph_size: Option<usize>,
    pub payload_size: usize,
    pub timeout: u64,
    pub min_delay: u64,
    pub max_delay: Option<u64>,
    pub chunk_size: usize,
    pub header_size: usize,
    pub max_pit_links: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum RateLimitingConfig {
    #[default]
    #[serde(rename = "no_limit")]
    NoLimit,
    #[serde(rename = "soft_limit")]
    SoftLimit { soft_limit: u64, soft_delay: u64 },
    #[serde(rename = "hard_limit")]
    HardLimit {
        hard_limit: u64,
        drop_behavior: DropBehavior,
    },
    #[serde(rename = "soft_with_hard_limit")]
    SoftWithHardLimit {
        soft_limit: u64,
        hard_limit: u64,
        soft_delay: u64,
        drop_behavior: DropBehavior,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct OpenTelemetryConfig {
    pub endpoint: String,
    pub service_name: String,
    pub logs: bool,
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
        .build()?;

    Ok(config.try_deserialize()?)
}
