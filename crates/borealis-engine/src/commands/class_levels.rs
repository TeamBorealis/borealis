//! Dividing a character's levels between its classes without changing the total (B2a, #34): a
//! character created at level 9 can become Fighter 5 / Wizard 4 without levelling up one at a time.
//!
//! The level list keeps the shape levelling up builds: moved levels leave from the end of the
//! class that gives them and join the end of the character's level list, so Fighter 9 split into
//! Wizard 4 is `[Fighter x5, Wizard x4]`, exactly what Fighter 5 followed by four Wizard level ups
//! stores. A multiclass is keyed by the character level of its first level, so a move renumbers
//! the multiclasses after it: their class pick in the `Multiclass (Level n)` slot and their hit
//! point rolls move with them. Choices made at the moved levels are dropped by the usual prune.

use super::selection::{prune, select};
use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::ids;
use crate::state::{CharacterState, Choice, ClassSlot, HpRolls, Selection, SlotHistory, SlotKey};
use borealis_content::{Rule, Ruleset};
use std::collections::{BTreeMap, BTreeSet};

/// Placeholder key of the multiclass a split creates (real multiclasses start at level 2 or
/// later); it is renumbered before the state leaves this module.
const NEW: ClassSlot = ClassSlot::Multi { starting_level: 0 };

/// Move `levels` of the main class's latest levels into a new multiclass; the character's total
/// level does not change. With `class` (a `Multiclass` element id) the class is picked in the new
/// multiclass's `Multiclass (Level n)` slot, checked as a pick there is (multiclass prerequisites,
/// or the character's "ignore multiclass prerequisites" house rule); without it the slot is left
/// for the picker, as a new multiclass is.
///
/// # Errors
/// [`CommandError::Level`] when multiclassing is not available, `levels` is 0 or would leave the
/// main class without a level; the pick's errors from [`select`].
pub fn split_new_multiclass(
    engine: &Engine,
    state: &CharacterState,
    levels: u32,
    class: Option<&str>,
) -> CmdResult {
    if !engine.derive_with(state, false).can_multiclass {
        return Err(CommandError::Level(
            "multiclassing is not available for this character".into(),
        ));
    }
    if levels == 0 {
        return Err(CommandError::Level(
            "a new multiclass needs at least one level".into(),
        ));
    }
    check_main_keeps_a_level(state, levels)?;
    let (moved, starts) = move_levels(engine, state, &ClassSlot::Main, &NEW, levels);
    let next = prune(engine, &moved);
    let Some(class) = class else {
        return Ok(CommandOutcome::new(next));
    };
    let start = starts
        .get(&0)
        .copied()
        .ok_or_else(|| CommandError::Level("the new multiclass has no starting level".into()))?;
    let rule = multiclass_rule(engine, state.ruleset, start).ok_or(CommandError::UnknownSlot)?;
    let key = SlotKey::new(ids::level_id(start), rule, 1);
    select(engine, &next, &key, Choice::Element(class.to_string()))
}

/// Give the multiclass that starts at `starting_level` exactly `levels` levels, taking them from
/// the main class or giving them back to it; the character's total level does not change. With
/// `levels` 0 the multiclass is removed and all its levels return to the main class.
///
/// # Errors
/// [`CommandError::Level`] for an unknown multiclass, or when the main class would be left
/// without a level.
pub fn set_multiclass_levels(
    engine: &Engine,
    state: &CharacterState,
    starting_level: u32,
    levels: u32,
) -> CmdResult {
    let slot = ClassSlot::Multi { starting_level };
    let current = state.class_level(&slot);
    if current == 0 {
        return Err(CommandError::Level("no such multiclass".into()));
    }
    let moved = match levels.cmp(&current) {
        std::cmp::Ordering::Equal => return Ok(CommandOutcome::new(state.clone())),
        std::cmp::Ordering::Greater => {
            let more = levels - current;
            check_main_keeps_a_level(state, more)?;
            move_levels(engine, state, &ClassSlot::Main, &slot, more).0
        }
        std::cmp::Ordering::Less => {
            move_levels(engine, state, &slot, &ClassSlot::Main, current - levels).0
        }
    };
    Ok(CommandOutcome::new(prune(engine, &moved)))
}

fn check_main_keeps_a_level(state: &CharacterState, taken: u32) -> Result<(), CommandError> {
    let main = state.class_level(&ClassSlot::Main);
    if taken >= main {
        return Err(CommandError::Level(format!(
            "the main class has {main} levels and keeps at least one"
        )));
    }
    Ok(())
}

/// Index of the `Multiclass` select rule of the level element `ID_LEVEL_n` (the slot that holds a
/// multiclass's class pick).
fn multiclass_rule(engine: &Engine, ruleset: Ruleset, n: u32) -> Option<u32> {
    let level = engine.universe(ruleset).get(&ids::level_id(n))?;
    let index = level
        .el
        .rules
        .iter()
        .position(|r| matches!(r, Rule::Select(s) if s.ty == "Multiclass"))?;
    u32::try_from(index).ok()
}

/// `state` with the last `n` levels of `from` moved to the end of the level list as levels of
/// `to`, every multiclass renumbered to its new first level. Also returns the renumbering (old
/// starting level to new; [`NEW`]'s is 0). Not pruned.
fn move_levels(
    engine: &Engine,
    state: &CharacterState,
    from: &ClassSlot,
    to: &ClassSlot,
    n: u32,
) -> (CharacterState, BTreeMap<u32, u32>) {
    let kept = without_last(&state.levels, from, n);
    let levels: Vec<ClassSlot> = kept
        .into_iter()
        .chain(std::iter::repeat_n(to.clone(), n as usize))
        .collect();
    let starts = new_starts(&levels);
    let renumber = |slot: &ClassSlot| match slot {
        ClassSlot::Multi { starting_level } => starts
            .get(starting_level)
            .map(|s| ClassSlot::Multi { starting_level: *s }),
        ClassSlot::Main => Some(ClassSlot::Main),
    };
    let remaining = levels.iter().filter(|l| *l == from).count();
    let hp_rolls = state
        .hp_rolls
        .iter()
        .filter_map(|r| {
            let rolls = if &r.class == from {
                r.rolls.iter().copied().take(remaining).collect()
            } else {
                r.rolls.clone()
            };
            let class = renumber(&r.class)?;
            // 0 is the "no roll" marker: an entry of nothing but those says nothing
            rolls
                .iter()
                .any(|r| *r != 0)
                .then_some(HpRolls { class, rolls })
        })
        .collect();
    let rekey = MulticlassKeys::new(engine, state, &starts);
    let next = CharacterState {
        levels: levels.iter().filter_map(renumber).collect(),
        hp_rolls,
        selections: state
            .selections
            .iter()
            .filter_map(|s| {
                let slot = rekey.apply(&s.slot)?;
                Some(Selection { slot, ..s.clone() })
            })
            .collect(),
        slot_history: state
            .slot_history
            .iter()
            .filter_map(|h| {
                let slot = rekey.apply(&h.slot)?;
                Some(SlotHistory { slot, ..h.clone() })
            })
            .collect(),
        ..state.clone()
    };
    (next, starts)
}

/// `levels` without the last `n` entries equal to `slot`.
fn without_last(levels: &[ClassSlot], slot: &ClassSlot, n: u32) -> Vec<ClassSlot> {
    let mut skip = n;
    let mut kept: Vec<ClassSlot> = levels
        .iter()
        .rev()
        .filter(|l| {
            if skip > 0 && *l == slot {
                skip -= 1;
                false
            } else {
                true
            }
        })
        .cloned()
        .collect();
    kept.reverse();
    kept
}

/// Old starting level of each multiclass in `levels` to the character level of its first entry.
fn new_starts(levels: &[ClassSlot]) -> BTreeMap<u32, u32> {
    let mut starts = BTreeMap::new();
    for (i, l) in levels.iter().enumerate() {
        if let ClassSlot::Multi { starting_level } = l {
            let level = u32::try_from(i + 1).unwrap_or(u32::MAX);
            starts.entry(*starting_level).or_insert(level);
        }
    }
    starts
}

/// The `Multiclass (Level n)` slot keys of the state's multiclasses, old and new.
struct MulticlassKeys {
    /// Old slot (owner, rule) to the new one; `None` for a multiclass that no longer exists.
    moves: BTreeMap<(String, u32), Option<(String, u32)>>,
}

impl MulticlassKeys {
    fn new(engine: &Engine, state: &CharacterState, starts: &BTreeMap<u32, u32>) -> Self {
        let key = |n: u32| multiclass_rule(engine, state.ruleset, n).map(|r| (ids::level_id(n), r));
        let moves = state
            .levels
            .iter()
            .filter_map(|l| match l {
                ClassSlot::Multi { starting_level } => Some(*starting_level),
                ClassSlot::Main => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|old| {
                let from = key(old)?;
                Some((from, starts.get(&old).and_then(|new| key(*new))))
            })
            .collect();
        Self { moves }
    }

    /// The key a stored choice moves to; `None` drops it (the pick of a removed multiclass).
    fn apply(&self, slot: &SlotKey) -> Option<SlotKey> {
        match self.moves.get(&(slot.owner.clone(), slot.rule)) {
            None => Some(slot.clone()),
            Some(None) => None,
            Some(Some((owner, rule))) => Some(SlotKey {
                owner: owner.clone(),
                rule: *rule,
                ..slot.clone()
            }),
        }
    }
}
