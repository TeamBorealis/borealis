//! Play tab spell tracker commands (GitHub #2, decision U13): mark spell slots and Pact Magic
//! slots used, pick or end concentration, and add, edit, track and delete the user's own
//! trackers. They only touch the play state of the character; rests are in [`super::play`].

use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::spell_tracker::{concentration_spells, slot_pools};
use crate::state::{CharacterState, CustomTracker, PlayState, TrackerReset};

/// Longest tracker name, in characters.
pub const MAX_TRACKER_NAME: usize = 60;
/// Highest tracker maximum.
pub const MAX_TRACKER_MAX: u32 = 999;
/// Most trackers on one character.
pub const MAX_TRACKERS: usize = 50;

const TRACKER_ID_PREFIX: &str = "tracker-";

/// A slot pool of the spell slots card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotPool {
    /// Normal slots of a spell level (1..=9).
    Level(u32),
    /// Pact Magic slots.
    Pact,
}

/// What the user enters for a tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerInput {
    /// Name (trimmed; 1..=[`MAX_TRACKER_NAME`] characters, no control characters).
    pub name: String,
    /// Maximum, 1..=[`MAX_TRACKER_MAX`].
    pub max: u32,
    /// When it resets.
    pub reset: TrackerReset,
}

fn with_play(state: &CharacterState, play: PlayState) -> CommandOutcome {
    CommandOutcome::new(CharacterState {
        play,
        ..state.clone()
    })
}

/// Mark `used` slots of `pool` as used (0 = all back).
///
/// # Errors
/// [`CommandError::Spell`] when the character has no slots in that pool or `used` is above the
/// number of slots.
pub fn set_spell_slots_used(
    engine: &Engine,
    state: &CharacterState,
    pool: SlotPool,
    used: u32,
) -> CmdResult {
    let pools = slot_pools(&engine.derive_with(state, false));
    let (max, label) = match pool {
        SlotPool::Level(l) => (pools.max_of(l), format!("level {l} spell slots")),
        SlotPool::Pact => (
            pools.pact.map_or(0, |p| p.count),
            "Pact Magic slots".to_string(),
        ),
    };
    if max == 0 {
        return Err(CommandError::Spell(format!("the character has no {label}")));
    }
    if used > max {
        return Err(CommandError::Spell(format!(
            "the character has {max} {label}"
        )));
    }
    let mut play = state.play.clone();
    match pool {
        SlotPool::Level(l) if used == 0 => {
            play.spell_slots_used.remove(&l);
        }
        SlotPool::Level(l) => {
            play.spell_slots_used.insert(l, used);
        }
        SlotPool::Pact => play.pact_slots_used = used,
    }
    Ok(with_play(state, play))
}

/// Concentrate on `spell` (replacing any other) or, with `None`, end concentration.
///
/// # Errors
/// [`CommandError::Spell`] when the spell is not a concentration spell the character knows or
/// has prepared.
pub fn set_concentration(
    engine: &Engine,
    state: &CharacterState,
    spell: Option<&str>,
) -> CmdResult {
    if let Some(id) = spell {
        let snap = engine.derive_with(state, false);
        if !concentration_spells(engine, state, &snap)
            .iter()
            .any(|s| s.id == id)
        {
            return Err(CommandError::Spell(format!(
                "{id} is not a concentration spell this character knows or has prepared"
            )));
        }
    }
    let play = PlayState {
        concentration: spell.map(str::to_string),
        ..state.play.clone()
    };
    Ok(with_play(state, play))
}

/// The trimmed, checked name and maximum of `input`.
fn checked(input: &TrackerInput) -> Result<(String, u32), CommandError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CommandError::Tracker("give the tracker a name".into()));
    }
    if name.chars().count() > MAX_TRACKER_NAME {
        return Err(CommandError::Tracker(format!(
            "a tracker name has at most {MAX_TRACKER_NAME} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(CommandError::Tracker(
            "a tracker name cannot hold control characters".into(),
        ));
    }
    if !(1..=MAX_TRACKER_MAX).contains(&input.max) {
        return Err(CommandError::Tracker(format!(
            "a tracker maximum is between 1 and {MAX_TRACKER_MAX}"
        )));
    }
    Ok((name.to_string(), input.max))
}

/// The next free `tracker-<n>` id (one above the highest in use).
fn next_id(trackers: &[CustomTracker]) -> String {
    let top = trackers
        .iter()
        .filter_map(|t| t.id.strip_prefix(TRACKER_ID_PREFIX)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{TRACKER_ID_PREFIX}{}", top.saturating_add(1))
}

fn position(state: &CharacterState, id: &str) -> Result<usize, CommandError> {
    state
        .play
        .trackers
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| CommandError::UnknownElement(id.to_string()))
}

/// Add a tracker (nothing used).
///
/// # Errors
/// [`CommandError::Tracker`] for a bad name or maximum, or when the character already has
/// [`MAX_TRACKERS`] trackers.
pub fn add_tracker(state: &CharacterState, input: &TrackerInput) -> CmdResult {
    let (name, max) = checked(input)?;
    if state.play.trackers.len() >= MAX_TRACKERS {
        return Err(CommandError::Tracker(format!(
            "a character has at most {MAX_TRACKERS} trackers"
        )));
    }
    let mut play = state.play.clone();
    play.trackers.push(CustomTracker {
        id: next_id(&play.trackers),
        name,
        max,
        used: 0,
        reset: input.reset,
    });
    Ok(with_play(state, play))
}

/// Rename a tracker, change its maximum (the used count is clamped) or its reset.
///
/// # Errors
/// [`CommandError::UnknownElement`] for an unknown id, [`CommandError::Tracker`] for a bad name or
/// maximum.
pub fn update_tracker(state: &CharacterState, id: &str, input: &TrackerInput) -> CmdResult {
    let at = position(state, id)?;
    let (name, max) = checked(input)?;
    let mut play = state.play.clone();
    let old = &play.trackers[at];
    play.trackers[at] = CustomTracker {
        id: old.id.clone(),
        name,
        max,
        used: old.used.min(max),
        reset: input.reset,
    };
    Ok(with_play(state, play))
}

/// Delete a tracker.
///
/// # Errors
/// [`CommandError::UnknownElement`] for an unknown id.
pub fn remove_tracker(state: &CharacterState, id: &str) -> CmdResult {
    let at = position(state, id)?;
    let mut play = state.play.clone();
    play.trackers.remove(at);
    Ok(with_play(state, play))
}

/// Set how many pips of a tracker are used.
///
/// # Errors
/// [`CommandError::UnknownElement`] for an unknown id, [`CommandError::Tracker`] above its maximum.
pub fn set_tracker_used(state: &CharacterState, id: &str, used: u32) -> CmdResult {
    let at = position(state, id)?;
    let mut play = state.play.clone();
    let t = &mut play.trackers[at];
    if used > t.max {
        return Err(CommandError::Tracker(format!(
            "{} has {} pips",
            t.name, t.max
        )));
    }
    t.used = used;
    Ok(with_play(state, play))
}
