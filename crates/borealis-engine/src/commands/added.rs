//! B3 (GitHub #35): add and remove the extra picks a DM grants ([`crate::added_picks`]).

use super::gear::{hide_line, next_uid, remove_item};
use super::selection::prune;
use super::{CmdResult, CommandError, CommandOutcome};
use crate::added_picks::{proxy_for, AddedKind};
use crate::engine::Engine;
use crate::state::{CharacterState, EquipSlot, InventoryItem};

/// Add element `id` as a pick of `kind` (a spell to spellcasting class `spellcasting`): a new
/// equipped, hidden line of its proxy item, marked as an added pick.
///
/// # Errors
/// [`CommandError::UnknownElement`] for an element that is not installed,
/// [`CommandError::NotAllowed`] for one the Add action does not offer
/// ([`Engine::added_pick_options`]), [`CommandError::Spell`] for a spell without a class.
pub fn add_pick(
    engine: &Engine,
    state: &CharacterState,
    kind: AddedKind,
    id: &str,
    spellcasting: Option<&str>,
) -> CmdResult {
    if kind == AddedKind::Spell && spellcasting.is_none() {
        return Err(CommandError::Spell(
            "an added spell belongs to a spellcasting class".into(),
        ));
    }
    if !engine.has_element(state.ruleset, id) {
        return Err(CommandError::UnknownElement(id.to_string()));
    }
    let offered = engine
        .added_pick_options(state, kind, spellcasting)
        .iter()
        .any(|o| o.id == id);
    let proxy = proxy_for(engine.universe(state.ruleset), kind, id, spellcasting)
        .filter(|_| offered)
        .ok_or_else(|| CommandError::NotAllowed(id.to_string()))?;
    let mut line = InventoryItem::new(next_uid(state), proxy);
    hide_line(&mut line);
    line.equipped = Some(EquipSlot::Worn);
    line.added_pick = true;
    let mut next = state.clone();
    next.inventory.items.push(line);
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Remove the added pick held by inventory line `uid`.
///
/// # Errors
/// [`CommandError::Inventory`] when `uid` is not an added pick's line.
pub fn remove_pick(engine: &Engine, state: &CharacterState, uid: &str) -> CmdResult {
    if !state
        .inventory
        .items
        .iter()
        .any(|i| i.uid == uid && i.added_pick)
    {
        return Err(CommandError::Inventory(format!("no added pick {uid}")));
    }
    remove_item(engine, state, uid)
}
