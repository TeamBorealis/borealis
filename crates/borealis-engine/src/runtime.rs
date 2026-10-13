//! Mutable play-time state of a character (resource uses, toggles, exhaustion, Heroic
//! Inspiration). Stored in the typed [`PlayState`] field `CharacterState.play`; `Runtime` is the
//! editing view over it.

use crate::state::{CharacterState, PlayState};
use std::collections::{BTreeMap, BTreeSet};

/// Legacy (format v1) reserved `PreparedSpells.class` key; only migrations still mention it.
pub const RUNTIME_KEY: &str = "#state";

/// Parsed runtime state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Runtime {
    /// Expended uses per resource id.
    pub used: BTreeMap<String, u32>,
    /// Active toggle ids.
    pub toggles: BTreeSet<String>,
    /// Exhaustion level.
    pub exhaustion: u32,
    /// Holds Inspiration.
    pub inspiration: bool,
}

impl Runtime {
    /// Read the play state of a character.
    #[must_use]
    pub fn from_state(state: &CharacterState) -> Self {
        let p = &state.play;
        Self {
            used: p.resources_used.clone(),
            toggles: p.toggles.clone(),
            exhaustion: p.exhaustion,
            inspiration: p.heroic_inspiration,
        }
    }

    /// A copy of `state` carrying this runtime.
    #[must_use]
    pub fn apply(&self, state: &CharacterState) -> CharacterState {
        let mut next = state.clone();
        next.play = PlayState {
            resources_used: self
                .used
                .iter()
                .filter(|(_, n)| **n > 0)
                .map(|(k, n)| (k.clone(), *n))
                .collect(),
            toggles: self.toggles.clone(),
            exhaustion: self.exhaustion,
            heroic_inspiration: self.inspiration,
            ..state.play.clone()
        };
        next
    }

    /// Expended uses of a resource.
    #[must_use]
    pub fn used_of(&self, id: &str) -> u32 {
        self.used.get(id).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_state() {
        let mut rt = Runtime::default();
        rt.used.insert("rage".into(), 2);
        rt.toggles.insert("rage".into());
        rt.exhaustion = 3;
        rt.inspiration = true;
        let state = rt.apply(&CharacterState::default());
        assert_eq!(Runtime::from_state(&state), rt);
        let cleared = Runtime::default().apply(&state);
        assert!(cleared.play.is_empty());
        assert!(cleared.prepared.is_empty());
    }
}
