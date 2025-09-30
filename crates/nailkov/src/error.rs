use rand::seq::WeightError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NailError {
    EmptyInput,
    BuildError(WeightError),
}

impl From<WeightError> for NailError {
    fn from(value: WeightError) -> Self {
        Self::BuildError(value)
    }
}

impl core::fmt::Display for NailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NailError::EmptyInput => write!(f, "Text input is empty"),
            NailError::BuildError(e) => {
                write!(f, "Failed to build token weight distributions: {e}")
            }
        }
    }
}

impl core::error::Error for NailError {}
