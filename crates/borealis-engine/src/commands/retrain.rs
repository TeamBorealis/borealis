//! Replace a learned spell (spell retraining on level-up).

use super::selection::{find_slot, prune};
use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::state::{CharacterState, ReplacedPick, SlotHistory, SlotKey};

/// Replace the spell in a filled slot: the old pick is unregistered (and unprepared), recorded
/// in the slot's history, and the slot reopens for a new pick at the current class level.
///
/// # Errors
/// [`CommandError::UnknownSlot`] for an unknown slot, [`CommandError::Spell`] when the slot is
/// empty, holds no spell, or the swap rule (profile, class and rule settings) forbids it.
pub fn replace_spell(engine: &Engine, state: &CharacterState, key: &SlotKey) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let slot = snap
        .slots
        .iter()
        .find(|s| &s.key == key && s.rejected.is_none())
        .ok_or(CommandError::UnknownSlot)?;
    if slot.spellcasting.is_none() {
        return Err(CommandError::Spell(
            "That slot does not hold a spell.".into(),
        ));
    }
    let chosen = slot
        .chosen
        .clone()
        .ok_or_else(|| CommandError::Spell("No spell is chosen in that slot.".into()))?;
    if !slot.replaceable {
        return Err(CommandError::Spell(format!(
            "{} cannot be replaced.",
            chosen.name
        )));
    }
    let level = engine
        .with_evaluation(state, |ev| {
            find_slot(&ev.graph, key)
                .map(|(owner, ..)| ev.graph.mgr_level(ev.graph.nodes[owner].mgr))
        })
        .ok_or(CommandError::UnknownSlot)?;
    let mut next = state.clone();
    next.selections.retain(|s| &s.slot != key);
    for list in &mut next.prepared {
        list.spells.retain(|s| s != &chosen.id);
    }
    let pick = ReplacedPick {
        id: chosen.id,
        level: level.max(slot.level),
    };
    match next.slot_history.iter_mut().find(|h| &h.slot == key) {
        Some(h) => h.replaced.push(pick),
        None => next.slot_history.push(SlotHistory {
            slot: key.clone(),
            replaced: vec![pick],
        }),
    }
    Ok(CommandOutcome::new(prune(engine, &next)))
}
