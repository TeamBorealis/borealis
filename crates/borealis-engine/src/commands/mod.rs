//! Pure command functions: each takes a state and returns a new one (nothing is mutated).

mod abilities;
mod forms;
mod gear;
mod improvement;
mod play;
mod progression;
mod retrain;
mod selection;

pub use abilities::*;
pub use forms::*;
pub use gear::*;
pub use improvement::*;
pub use play::*;
pub use progression::*;
pub use retrain::*;
pub use selection::*;

use crate::state::CharacterState;
use thiserror::Error;

/// Why a command was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    /// The slot does not exist (or is not open) for this character.
    #[error("no such selection slot")]
    UnknownSlot,
    /// The element does not exist in the installed content.
    #[error("unknown element {0}")]
    UnknownElement(String),
    /// The element is not offered by the slot.
    #[error("{0} is not an allowed choice for this slot")]
    NotAllowed(String),
    /// Level limits.
    #[error("{0}")]
    Level(String),
    /// Ability score problems.
    #[error("{0}")]
    Ability(String),
    /// Inventory problems.
    #[error("{0}")]
    Inventory(String),
    /// Money problems.
    #[error("{0}")]
    Money(String),
    /// Spell preparation problems.
    #[error("{0}")]
    Spell(String),
}

/// A successful command: the new state and any non-fatal warnings for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutcome {
    /// The resulting state.
    pub state: CharacterState,
    /// Warnings such as "equipped without attunement".
    pub warnings: Vec<String>,
}

impl CommandOutcome {
    pub(crate) fn new(state: CharacterState) -> Self {
        Self {
            state,
            warnings: Vec::new(),
        }
    }
}

/// Result of a command.
pub type CmdResult = Result<CommandOutcome, CommandError>;
