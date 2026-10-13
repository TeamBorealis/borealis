//! Levels, experience, options, hit point rolls and spell preparation.

use super::selection::prune;
use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::ids;
use crate::state::{CharacterState, ClassSlot, HpRolls, PreparedSpells};
use borealis_content::{ElementId, Ruleset};

const MAX_LEVEL: u32 = 20;

/// A fresh level-1 character with the default options (the average hit points option follows
/// `EngineSettings::default_average_hp`).
#[must_use]
pub fn new_character(engine: &Engine, ruleset: Ruleset) -> CharacterState {
    let mut s = CharacterState {
        ruleset,
        ..CharacterState::default()
    };
    s.options = engine
        .default_options(ruleset)
        .into_iter()
        .filter(|o| o != ids::OPT_AVERAGE_HP)
        .collect();
    if engine.settings().default_average_hp && engine.has_element(ruleset, ids::OPT_AVERAGE_HP) {
        s.options.push(ids::OPT_AVERAGE_HP.to_string());
    }
    s
}

fn record_roll(state: &mut CharacterState, slot: &ClassSlot, class_level: u32, roll: Option<u32>) {
    let Some(v) = roll else { return };
    let idx = class_level.saturating_sub(1) as usize;
    let entry = match state.hp_rolls.iter().position(|r| &r.class == slot) {
        Some(i) => i,
        None => {
            state.hp_rolls.push(HpRolls {
                class: slot.clone(),
                rolls: Vec::new(),
            });
            state.hp_rolls.len() - 1
        }
    };
    let rolls = &mut state.hp_rolls[entry].rolls;
    if rolls.len() <= idx {
        rolls.resize(idx + 1, 0);
    }
    rolls[idx] = v;
}

fn level_up(
    engine: &Engine,
    state: &CharacterState,
    slot: ClassSlot,
    roll: Option<u32>,
) -> CmdResult {
    let new_level = state.level() + 1;
    if new_level > MAX_LEVEL {
        return Err(CommandError::Level("the maximum level is 20".into()));
    }
    let mut next = state.clone();
    next.levels.push(slot.clone());
    next.experience = next
        .experience
        .max(engine.xp_threshold(state.ruleset, new_level));
    let class_level = next.class_level(&slot);
    record_roll(&mut next, &slot, class_level, roll);
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Gain a level in the main class (`hp_roll` is stored when hit points are rolled).
///
/// # Errors
/// [`CommandError::Level`] at level 20.
pub fn level_up_main(engine: &Engine, state: &CharacterState, hp_roll: Option<u32>) -> CmdResult {
    level_up(engine, state, ClassSlot::Main, hp_roll)
}

/// Start a multiclass: the new character level belongs to a class chosen in the
/// `Multiclass (Level n)` slot.
///
/// # Errors
/// [`CommandError::Level`] at level 20 or when multiclassing is not available.
pub fn level_up_new_multiclass(
    engine: &Engine,
    state: &CharacterState,
    hp_roll: Option<u32>,
) -> CmdResult {
    if !engine.derive_with(state, false).can_multiclass {
        return Err(CommandError::Level(
            "multiclassing is not available for this character".into(),
        ));
    }
    let start = state.level() + 1;
    level_up(
        engine,
        state,
        ClassSlot::Multi {
            starting_level: start,
        },
        hp_roll,
    )
}

/// Gain a level in an existing multiclass identified by its starting level.
///
/// # Errors
/// [`CommandError::Level`] at level 20 or for an unknown multiclass.
pub fn level_up_multiclass(
    engine: &Engine,
    state: &CharacterState,
    starting_level: u32,
    hp_roll: Option<u32>,
) -> CmdResult {
    let slot = ClassSlot::Multi { starting_level };
    if !state.levels.contains(&slot) {
        return Err(CommandError::Level("no such multiclass".into()));
    }
    level_up(engine, state, slot, hp_roll)
}

/// Remove the last level. Experience becomes the threshold of the new level.
///
/// # Errors
/// [`CommandError::Level`] at level 1.
pub fn level_down(engine: &Engine, state: &CharacterState) -> CmdResult {
    if state.levels.len() <= 1 {
        return Err(CommandError::Level(
            "a character cannot go below level 1".into(),
        ));
    }
    let mut next = state.clone();
    let Some(removed) = next.levels.pop() else {
        return Err(CommandError::Level(
            "a character cannot go below level 1".into(),
        ));
    };
    next.experience = engine.xp_threshold(state.ruleset, next.level());
    let remaining = next.class_level(&removed);
    if let Some(r) = next.hp_rolls.iter_mut().find(|r| r.class == removed) {
        r.rolls.truncate(remaining as usize);
    }
    next.hp_rolls.retain(|r| !r.rolls.is_empty());
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Set experience (levels are never changed automatically).
#[must_use]
pub fn set_experience(state: &CharacterState, xp: u32) -> CharacterState {
    CharacterState {
        experience: xp,
        ..state.clone()
    }
}

/// Enable or disable an Option element (feats, multiclassing, homebrew options).
///
/// # Errors
/// [`CommandError::UnknownElement`] for ids that are not installed Option elements.
pub fn set_option(engine: &Engine, state: &CharacterState, id: &str, enabled: bool) -> CmdResult {
    if enabled && !engine.has_element(state.ruleset, id) {
        return Err(CommandError::UnknownElement(id.to_string()));
    }
    let mut next = state.clone();
    next.options.retain(|o| o != id);
    if enabled {
        next.options.push(id.to_string());
    }
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Store one hit point roll (`class_level` is 1-based; the main class's level 1 is always the die).
///
/// # Errors
/// [`CommandError::Level`] if the value is zero or the level exceeds 20.
pub fn set_hp_roll(
    state: &CharacterState,
    slot: &ClassSlot,
    class_level: u32,
    value: u32,
) -> CmdResult {
    if value == 0 || class_level == 0 || class_level > MAX_LEVEL {
        return Err(CommandError::Level(
            "hit point rolls start at 1 and levels at 1..=20".into(),
        ));
    }
    let mut next = state.clone();
    record_roll(&mut next, slot, class_level, Some(value));
    Ok(CommandOutcome::new(next))
}

/// Mark a spell prepared or not. Enforces the prepared limit (at least 1); always-prepared
/// spells and cantrips cannot be toggled.
///
/// # Errors
/// [`CommandError::Spell`] for unknown classes, spells, cantrips, always-prepared spells or a full list.
pub fn set_prepared(
    engine: &Engine,
    state: &CharacterState,
    class: &str,
    spell: &str,
    prepared: bool,
) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let caster = snap
        .caster(class)
        .ok_or_else(|| CommandError::Spell(format!("{class} does not cast spells")))?;
    if !caster.prepare {
        return Err(CommandError::Spell(format!(
            "{class} does not prepare spells"
        )));
    }
    let entry = caster
        .spells
        .iter()
        .find(|s| s.id == spell)
        .ok_or_else(|| CommandError::Spell("the spell is not available to this class".into()))?;
    if entry.level == 0 || entry.always_prepared {
        return Err(CommandError::Spell(
            "cantrips and always-prepared spells cannot be toggled".into(),
        ));
    }
    if prepared && !entry.prepared {
        let limit = caster.prepare_limit.unwrap_or(i32::MAX);
        if i64::from(caster.prepared_count) >= i64::from(limit) {
            return Err(CommandError::Spell(format!(
                "{class} can prepare at most {limit} spells"
            )));
        }
    }
    let mut next = state.clone();
    let list = match next.prepared.iter().position(|p| p.class == class) {
        Some(i) => i,
        None => {
            next.prepared.push(PreparedSpells {
                class: class.to_string(),
                spells: Vec::new(),
            });
            next.prepared.len() - 1
        }
    };
    next.prepared[list].spells.retain(|s| s != spell);
    if prepared {
        next.prepared[list].spells.push(spell.to_string());
    }
    Ok(CommandOutcome::new(next))
}

/// Copy a spell into (or remove it from) a class spellbook (2024 `spellbook` rule). The spell must
/// be on the class list, of a level the class has slots for, and the book must have room.
/// Removing a spell also unprepares it.
///
/// # Errors
/// [`CommandError::Spell`] when the class has no spellbook, the spell is not a candidate, or the
/// book is full.
pub fn set_spellbook(
    engine: &Engine,
    state: &CharacterState,
    class: &str,
    spell: &str,
    in_book: bool,
) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let book = snap
        .caster(class)
        .and_then(|c| c.spellbook.as_ref())
        .ok_or_else(|| CommandError::Spell(format!("{class} has no spellbook")))?;
    let present = book.spells.iter().any(|s| s.id == spell);
    if in_book && !present {
        if !book.candidates.iter().any(|s| s.id == spell) {
            return Err(CommandError::Spell(
                "the spell is not on this class list or is too high a level".into(),
            ));
        }
        if book.spells.len() >= book.capacity as usize {
            return Err(CommandError::Spell(format!(
                "the {class} spellbook holds at most {} spells",
                book.capacity
            )));
        }
    }
    if !in_book && !present {
        return Err(CommandError::Spell(
            "the spell is not in the spellbook".into(),
        ));
    }
    let mut next = state.clone();
    let book = next.spellbooks.entry(class.to_string()).or_default();
    book.retain(|s| s.0 != spell);
    if in_book {
        book.push(ElementId::new(spell));
    } else if let Some(p) = next.prepared.iter_mut().find(|p| p.class == class) {
        p.spells.retain(|s| s != spell);
    }
    Ok(CommandOutcome::new(next))
}
