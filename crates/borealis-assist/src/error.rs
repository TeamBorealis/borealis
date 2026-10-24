//! Errors of the assistant layer.

use borealis_ai::AiError;
use borealis_engine::commands::CommandError;

/// Why an assistant operation failed.
#[derive(Debug, thiserror::Error)]
pub enum AssistError {
    /// The model call failed (CLI missing, timeout, validation exhausted, ...).
    #[error(transparent)]
    Ai(#[from] AiError),
    /// An engine command refused an operation.
    #[error("{0}")]
    Engine(#[from] CommandError),
    /// Text could not be extracted from the supplied file.
    #[error("{0}")]
    Extraction(String),
    /// A level-up option could not be applied.
    #[error("{0}")]
    Invalid(String),
}
