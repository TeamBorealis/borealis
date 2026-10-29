//! Play tab spell tracker commands (GitHub #2, decision U13): spend and restore spell slots and
//! Pact Magic slots, pick or end concentration, and add, edit, spend, restore and delete the
//! user's own trackers. Uses are relative (one more, one fewer), like resources, so quick presses
//! and a press right after a rest are never lost and never undo the rest. They only touch the
//! play state; rests are in [`super::play`]. The slot and concentration commands read a derive of
//! `state` the caller already has (the host's cached one), so a press costs no extra derive.

use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::play_load::{clean_tracker_name, next_tracker_id};
use crate::snapshot::CharacterSnapshot;
use crate::spell_tracker::{concentration_spells, slot_pools};
use crate::state::{CharacterState, CustomTracker, PlayState, TrackerReset};

/// Longest tracker name, in characters.
pub const MAX_TRACKER_NAME: usize = 60;
/// Highest tracker maximum.
pub const MAX_TRACKER_MAX: u32 = 999;
/// Most trackers on one character.
pub const MAX_TRACKERS: usize = 50;
/// Longest id echoed back in an error message (review L5).
const MAX_ECHOED_ID: usize = 64;

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
    /// Name (bidi and zero-width characters stripped, trimmed; 1..=[`MAX_TRACKER_NAME`]
    /// characters, no control characters).
    pub name: String,
    /// Maximum, 1..=[`MAX_TRACKER_MAX`].
    pub max: u32,
    /// When it resets.
    pub reset: TrackerReset,
    /// Shown counting up (used/max) instead of uses left.
    pub counts_up: bool,
}

/// `id`, cut to [`MAX_ECHOED_ID`] characters for an error message.
fn echo(id: &str) -> String {
    if id.chars().count() <= MAX_ECHOED_ID {
        id.to_string()
    } else {
        format!("{}…", id.chars().take(MAX_ECHOED_ID).collect::<String>())
    }
}

fn with_play(state: &CharacterState, play: PlayState) -> CommandOutcome {
    CommandOutcome::new(CharacterState {
        play,
        ..state.clone()
    })
}

/// Slots of `pool` (0 = none), its label, and the used count clamped to it.
fn pool_of(snap: &CharacterSnapshot, state: &CharacterState, pool: SlotPool) -> (u32, String, u32) {
    let pools = slot_pools(snap);
    let (max, label, used) = match pool {
        SlotPool::Level(l) => (
            pools.max_of(l),
            format!("level {l} spell slots"),
            state.play.spell_slots_used.get(&l).copied().unwrap_or(0),
        ),
        SlotPool::Pact => (
            pools.pact.map_or(0, |p| p.count),
            "Pact Magic slots".to_string(),
            state.play.pact_slots_used,
        ),
    };
    (max, label, used.min(max))
}

fn with_slots_used(state: &CharacterState, pool: SlotPool, used: u32) -> CommandOutcome {
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
    with_play(state, play)
}

/// Mark `n` more slots of `pool` used. `snap` is a derive of `state`.
///
/// # Errors
/// [`CommandError::Spell`] when the character has no slots in that pool or fewer than `n` left.
pub fn spend_spell_slots(
    snap: &CharacterSnapshot,
    state: &CharacterState,
    pool: SlotPool,
    n: u32,
) -> CmdResult {
    let (max, label, used) = pool_of(snap, state, pool);
    if max == 0 {
        return Err(CommandError::Spell(format!("the character has no {label}")));
    }
    if n > max - used {
        return Err(CommandError::Spell(format!(
            "{} of {max} {label} left",
            max - used
        )));
    }
    Ok(with_slots_used(state, pool, used + n))
}

/// Give back `n` used slots of `pool` (never below none used). `snap` is a derive of `state`.
///
/// # Errors
/// [`CommandError::Spell`] when the character has no slots in that pool.
pub fn restore_spell_slots(
    snap: &CharacterSnapshot,
    state: &CharacterState,
    pool: SlotPool,
    n: u32,
) -> CmdResult {
    let (max, label, used) = pool_of(snap, state, pool);
    if max == 0 {
        return Err(CommandError::Spell(format!("the character has no {label}")));
    }
    Ok(with_slots_used(state, pool, used.saturating_sub(n)))
}

/// Concentrate on `spell` (replacing any other) or, with `None`, end concentration. `snap` is a
/// derive of `state`.
///
/// # Errors
/// [`CommandError::Spell`] when the spell is not a concentration spell the character knows or
/// has prepared.
pub fn set_concentration(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
    spell: Option<&str>,
) -> CmdResult {
    if let Some(id) = spell {
        if !concentration_spells(engine, state, snap)
            .iter()
            .any(|s| s.id == id)
        {
            return Err(CommandError::Spell(format!(
                "{} is not a concentration spell this character knows or has prepared",
                echo(id)
            )));
        }
    }
    let play = PlayState {
        concentration: spell.map(str::to_string),
        ..state.play.clone()
    };
    Ok(with_play(state, play))
}

/// The cleaned, checked name and maximum of `input`.
fn checked(input: &TrackerInput) -> Result<(String, u32), CommandError> {
    let name = clean_tracker_name(&input.name);
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
    Ok((name, input.max))
}

fn position(state: &CharacterState, id: &str) -> Result<usize, CommandError> {
    state
        .play
        .trackers
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| CommandError::UnknownElement(echo(id)))
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
        id: next_tracker_id(&play.trackers),
        name,
        max,
        used: 0,
        reset: input.reset.clone(),
        counts_up: input.counts_up,
        extra: std::collections::BTreeMap::new(),
    });
    Ok(with_play(state, play))
}

/// Rename a tracker, change its maximum (the used count is clamped), its reset or its direction.
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
        name,
        max,
        used: old.used.min(max),
        reset: input.reset.clone(),
        counts_up: input.counts_up,
        ..old.clone()
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

/// Mark `n` more pips of a tracker used (for a counting-up tracker: gained).
///
/// # Errors
/// [`CommandError::UnknownElement`] for an unknown id, [`CommandError::Level`] when fewer than
/// `n` pips are left.
pub fn spend_tracker(state: &CharacterState, id: &str, n: u32) -> CmdResult {
    let at = position(state, id)?;
    let mut play = state.play.clone();
    let t = &mut play.trackers[at];
    let used = t.used.min(t.max);
    if n > t.max - used {
        return Err(CommandError::Level(format!(
            "{} has {} of {} pips left",
            t.name,
            t.max - used,
            t.max
        )));
    }
    t.used = used + n;
    Ok(with_play(state, play))
}

/// Give back `n` used pips of a tracker (never below none used).
///
/// # Errors
/// [`CommandError::UnknownElement`] for an unknown id.
pub fn restore_tracker(state: &CharacterState, id: &str, n: u32) -> CmdResult {
    let at = position(state, id)?;
    let mut play = state.play.clone();
    let t = &mut play.trackers[at];
    t.used = t.used.min(t.max).saturating_sub(n);
    Ok(with_play(state, play))
}
