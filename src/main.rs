#![forbid(unsafe_code)]

use color_eyre::Result;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<()> {
    color_eyre::install()?;

    let config = nailconfig::get_configuration()?;

    let (inputs, interner) = nailpit::inputs::get_input_files(config.as_ref())?;

    let templates = nailpit::inputs::get_template_files(config.as_ref())?;

    let spicy = nailspicy::get_spicy_payload(config.as_ref());

    let state = nailstate::ServerState::new(config, inputs, interner, templates);

    nailpit::runtime::run(state, spicy)?;

    Ok(())
}
