use color_eyre::Result;
use serde_aux::field_attributes::{
    deserialize_number_from_string, deserialize_option_number_from_string,
};

#[derive(Debug, Default, serde::Deserialize)]
pub struct NailConfig {
    pub generator: GeneratorConfig,
}

#[derive(Debug, serde::Deserialize)]
pub struct GeneratorConfig {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub min_size: usize,
    #[serde(deserialize_with = "deserialize_option_number_from_string")]
    pub max_size: Option<usize>,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            min_size: 128,
            max_size: None,
        }
    }
}

pub fn get_configuration() -> Result<NailConfig> {
    let config_dir = std::env::current_dir()?;

    let config = config::Config::builder()
        .add_source(
            config::File::from(config_dir.join("pit.toml")).format(config::FileFormat::Toml),
        )
        .add_source(
            config::Environment::with_prefix("PIT")
                .prefix_separator("__")
                .separator("_"),
        )
        .build()?;

    Ok(config.try_deserialize().unwrap_or_default())
}
