//! Selection commands: choose, clear, prune and fill defaults.

use super::{CmdResult, CommandError, CommandOutcome};
use crate::derive::Evaluation;
use crate::engine::Engine;
use crate::graph::Graph;
use crate::snapshot::CharacterSnapshot;
use crate::state::{CharacterState, Choice, Selection, SlotKey};
use borealis_content::Rule;
use std::collections::HashSet;
use std::sync::Arc;

pub(super) fn find_slot<'g, 'u>(
    graph: &'g Graph<'u>,
    key: &SlotKey,
) -> Option<(usize, usize, usize)> {
    graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.alive)
        .find_map(|(i, n)| {
            n.slots
                .iter()
                .position(|s| &s.key == key)
                .map(|si| (i, si, n.slots[si].rule))
        })
}

/// A new state without selections that no longer belong to an active slot or are not allowed.
#[must_use]
pub fn prune(engine: &Engine, state: &CharacterState) -> CharacterState {
    let active = engine.with_evaluation(state, |ev| ev.active_slot_keys());
    retain_active(state, |k| active.contains(k))
}

/// [`prune`] given a derive of `state` (with or without option lists) made by the same engine:
/// the snapshot already lists the active slots, so nothing is evaluated again.
#[must_use]
pub fn prune_derived(state: &CharacterState, derived: &CharacterSnapshot) -> CharacterState {
    let active: HashSet<&SlotKey> = derived
        .slots
        .iter()
        .filter(|s| s.rejected.is_none())
        .map(|s| &s.key)
        .collect();
    retain_active(state, |k| active.contains(k))
}

/// [`prune`] as a command outcome; with `derive`, the outcome also carries the full derive of
/// the state when the prune dropped nothing (one evaluation serves both: a derive is a pure
/// function of the state, and the prune reads the same evaluation's slots).
pub(super) fn pruned(engine: &Engine, next: &CharacterState, derive: bool) -> CommandOutcome {
    if !derive {
        return CommandOutcome::new(prune(engine, next));
    }
    let ev = Evaluation::run(
        engine.universe(next.ruleset),
        next,
        engine.settings(),
        false,
    );
    let active = ev.active_slot_keys();
    let state = retain_active(next, |k| active.contains(k));
    let mut out = CommandOutcome::new(state);
    if out.state == *next {
        out.derived = Some(Arc::new(engine.finish_derive(ev, next)));
    }
    out
}

fn retain_active(state: &CharacterState, active: impl Fn(&SlotKey) -> bool) -> CharacterState {
    let mut next = state.clone();
    next.selections.retain(|s| active(&s.slot));
    next.slot_history.retain(|h| active(&h.slot));
    next
}

/// Choose an element (or list entry) for a slot. Dependent selections of whatever the slot held
/// before are dropped (cascade).
///
/// # Errors
/// [`CommandError::UnknownSlot`], [`CommandError::UnknownElement`] or [`CommandError::NotAllowed`].
pub fn select(engine: &Engine, state: &CharacterState, key: &SlotKey, choice: Choice) -> CmdResult {
    check_choice(engine, state, key, &choice)?;
    Ok(commit(engine, state, key, choice, false))
}

/// [`select`] given the full derive of `state` (option lists included) made by the same engine,
/// as the builder has it when the user picks. An element the snapshot lists as an option of the
/// open slot is valid without a second evaluation: the option list is the same per-candidate
/// check. Any other choice is checked as [`select`] does. The outcome carries the new state's
/// full derive when the prune dropped nothing ([`CommandOutcome::derived`]).
///
/// # Errors
/// As [`select`].
pub fn select_derived(
    engine: &Engine,
    state: &CharacterState,
    derived: &CharacterSnapshot,
    key: &SlotKey,
    choice: Choice,
) -> CmdResult {
    if !offered(derived, key, &choice) {
        check_choice(engine, state, key, &choice)?;
    }
    Ok(commit(engine, state, key, choice, true))
}

/// Does the snapshot's one open slot with this key list the element? A key that occurs twice is
/// left to the full check (it finds the first live node with that key).
fn offered(derived: &CharacterSnapshot, key: &SlotKey, choice: &Choice) -> bool {
    let Choice::Element(id) = choice else {
        return false;
    };
    let mut slots = derived.slots.iter().filter(|s| &s.key == key);
    let (Some(slot), None) = (slots.next(), slots.next()) else {
        return false;
    };
    slot.chosen.is_none() && slot.options.iter().any(|o| &o.id == id)
}

fn check_choice(
    engine: &Engine,
    state: &CharacterState,
    key: &SlotKey,
    choice: &Choice,
) -> Result<(), CommandError> {
    engine.with_evaluation(state, |ev| -> Result<(), CommandError> {
        let (owner, si, rule) = find_slot(&ev.graph, key).ok_or(CommandError::UnknownSlot)?;
        let Rule::Select(r) = &ev.graph.nodes[owner].e.el.rules[rule] else {
            return Err(CommandError::UnknownSlot);
        };
        match choice {
            Choice::ListItem(i) => {
                if r.is_list() && r.items.iter().any(|x| x.id == *i) {
                    Ok(())
                } else {
                    Err(CommandError::NotAllowed(i.to_string()))
                }
            }
            Choice::Element(id) => {
                let target = ev
                    .uni
                    .get(id)
                    .ok_or_else(|| CommandError::UnknownElement(id.clone()))?;
                let current = ev.graph.nodes[owner].slots[si]
                    .chosen
                    .map(|c| ev.graph.nodes[c].e.el.id.as_str());
                if current == Some(id.as_str())
                    || ev.graph.choice_allowed(owner, rule, target, false)
                {
                    Ok(())
                } else {
                    Err(CommandError::NotAllowed(id.clone()))
                }
            }
        }
    })
}

/// The state with `choice` in the slot, pruned (see [`pruned`] for `derive`).
fn commit(
    engine: &Engine,
    state: &CharacterState,
    key: &SlotKey,
    choice: Choice,
    derive: bool,
) -> CommandOutcome {
    let mut next = state.clone();
    next.selections.retain(|s| &s.slot != key);
    // a pick into a reopened slot is gained at the level the slot reopened at
    let replace_level = state.replaced_in(key).last().map(|p| p.level);
    next.selections.push(Selection {
        slot: key.clone(),
        choice,
        replace_level,
    });
    pruned(engine, &next, derive)
}

/// Remove the choice of a slot (and everything that depended on it).
///
/// # Errors
/// Never fails; unknown keys are ignored.
pub fn clear(engine: &Engine, state: &CharacterState, key: &SlotKey) -> CmdResult {
    let mut next = state.clone();
    next.selections.retain(|s| &s.slot != key);
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Fill open slots that name a `default` element (new character / level-up helper).
#[must_use]
pub fn apply_defaults(engine: &Engine, state: &CharacterState) -> CharacterState {
    let mut cur = state.clone();
    for _ in 0..40 {
        let snap = engine.derive(&cur);
        let pick = snap.slots.iter().find(|s| {
            s.chosen.is_none()
                && s.rejected.is_none()
                && s.missing.is_none()
                && s.default
                    .as_ref()
                    .is_some_and(|d| s.options.iter().any(|o| &o.id == d))
        });
        let Some(slot) = pick else { break };
        let id = slot.default.clone().unwrap_or_default();
        cur.selections.push(Selection {
            slot: slot.key.clone(),
            choice: Choice::Element(id),
            replace_level: None,
        });
    }
    cur
}

/// Find the key of a slot by owner element id, rule name and ordinal in a snapshot.
#[must_use]
pub fn slot_key(
    snapshot: &crate::snapshot::CharacterSnapshot,
    owner: &str,
    name: &str,
    ordinal: u32,
) -> Option<SlotKey> {
    snapshot
        .slots
        .iter()
        .find(|s| s.owner.id == owner && s.name == name && s.key.ordinal == ordinal)
        .map(|s| s.key.clone())
}

/// Mark a spell selection as swapped at the current character level (retrain marker).
///
/// # Errors
/// [`CommandError::UnknownSlot`] if no choice is stored for the slot.
pub fn set_replace_level(state: &CharacterState, key: &SlotKey, level: u32) -> CmdResult {
    let mut next = state.clone();
    let sel = next
        .selections
        .iter_mut()
        .find(|s| &s.slot == key)
        .ok_or(CommandError::UnknownSlot)?;
    sel.replace_level = Some(level);
    Ok(CommandOutcome::new(next))
}
