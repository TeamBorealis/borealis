//! Pure command functions: each takes a state and returns a new one (nothing is mutated).

mod abilities;
mod forms;
mod gear;
mod improvement;
mod play;
mod progression;
mod retrain;
mod selection;
mod version;

pub use abilities::*;
pub use forms::*;
pub use gear::*;
pub use improvement::*;
pub use play::*;
pub use progression::*;
pub use retrain::*;
pub use selection::*;
pub use version::*;

use crate::snapshot::CharacterSnapshot;
use crate::state::CharacterState;
use std::sync::Arc;
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
    /// A version switch was refused (no pick, or not a version of it; B-55).
    #[error("{0}")]
    Version(String),
}

/// A successful command: the new state and any non-fatal warnings for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutcome {
    /// The resulting state.
    pub state: CharacterState,
    /// Warnings such as "equipped without attunement".
    pub warnings: Vec<String>,
    /// [`crate::Engine::derive`] of `state`, when the command had it at hand (the `_derived`
    /// commands: their prune evaluated the state and dropped nothing).
    pub derived: Option<Arc<CharacterSnapshot>>,
}

impl CommandOutcome {
    pub(crate) fn new(state: CharacterState) -> Self {
        Self {
            state,
            warnings: Vec::new(),
            derived: None,
        }
    }
}

/// Result of a command.
pub type CmdResult = Result<CommandOutcome, CommandError>;
