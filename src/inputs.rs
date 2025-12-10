use std::{fs::read_to_string, sync::Arc};

use color_eyre::eyre::Context;
use glob::glob;
use nailbox::{arc_within, try_arc_within};
use nailconfig::NailConfig;
use nailgen::{GeneratedTemplate, MarkovGen, WarningTemplate};
use nailkov::interner::Interner;
use nailstate::Templates;

/// Takes a glob for finding all input files and returns a read-only list of
/// all markov chains that can be generated.
pub fn get_input_files(
    config: &NailConfig,
) -> color_eyre::Result<(Arc<[MarkovGen]>, Arc<Interner>)> {
    let mut interner = arc_within(|| Interner::with_capacity(512));

    let interned_mut = Arc::get_mut(&mut interner).unwrap();

    let inputs = glob(&config.generator.input_files)?
        .filter_map(|path| {
            path.inspect_err(|err| tracing::error!("IO Error: {err}"))
                .ok()
        })
        .filter_map(|input| {
            MarkovGen::new(input, interned_mut)
                .inspect_err(|err| tracing::error!("Markov Error: {err}"))
                .ok()
        })
        .collect::<Arc<[MarkovGen]>>();

    if inputs.is_empty() {
        color_eyre::eyre::bail!("No input files found! Exiting...");
    }

    Ok((inputs, interner))
}

pub fn get_template_files(config: &NailConfig) -> color_eyre::Result<Arc<Templates>> {
    let (index, content) = (
        read_to_string(&config.generator.warning_template).context("Missing Warning Template")?,
        read_to_string(&config.generator.warning_message).context("Missing Warning Message")?,
    );
    let generated = read_to_string(&config.generator.generated_template)
        .context("Missing Generated Template")?;

    let templates = try_arc_within(|| -> color_eyre::Result<Templates> {
        Ok(Templates {
            warning: WarningTemplate::init(index.into(), content.into()).context(
                "Warning Template should only contain title, main and footer placeholders"
            )?,
            generated: GeneratedTemplate::init(generated.into()).context("Generated template contains invalid placeholders, should only contain title, initial, main, extra and footer placeholders")?,
        })
    })?;

    Ok(templates)
}
