use color_eyre::Result;
use serde_aux::field_attributes::{
    deserialize_number_from_string, deserialize_option_number_from_string,
};

#[derive(Debug, serde::Deserialize)]
pub struct NailConfig {
    pub generator: GeneratorConfig,
}

#[derive(Debug, serde::Deserialize)]
pub struct GeneratorConfig {
    pub input_files: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub min_paragraph_size: usize,
    #[serde(deserialize_with = "deserialize_option_number_from_string")]
    pub max_paragraph_size: Option<usize>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub payload_size: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub timeout: u64,
}

pub fn get_configuration() -> Result<NailConfig> {
    let config_dir = std::env::current_dir()?.join("configuration");

    let config = config::Config::builder()
        .add_source(
            config::File::from(config_dir.join("pit.default.toml"))
                .format(config::FileFormat::Toml),
        )
        .add_source(
            config::File::from(config_dir.join("pit.toml")).format(config::FileFormat::Toml),
        )
        .add_source(
            config::Environment::with_prefix("PIT")
                .prefix_separator("__")
                .separator("__"),
        )
        .build()?;

    Ok(config.try_deserialize()?)
}
