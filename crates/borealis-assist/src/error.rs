//! Errors of the assistant layer.

use borealis_ai::AiError;
use borealis_encounter::EncounterError;
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
    /// The encounter math refused its input (bad party, missing 2014 tables, ...).
    #[error(transparent)]
    Encounter(#[from] EncounterError),
    /// Text could not be extracted from the supplied file.
    #[error("{0}")]
    Extraction(String),
    /// A level-up option or plan step could not be applied.
    #[error("{0}")]
    Invalid(String),
}
