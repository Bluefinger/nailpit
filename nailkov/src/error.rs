#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NailError {
    EmptyInput,
    BuildError,
}

impl core::fmt::Display for NailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NailError::EmptyInput => write!(f, "Text input is empty"),
            NailError::BuildError => write!(f, "Failed to build token weight distributions"),
        }
    }
}

impl core::error::Error for NailError {}
