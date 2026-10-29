//! Warnings for grants an equipped item cannot apply yet (Discord #3, GitHub issue 3).
//!
//! Like Aurora (`ProgressionManager`: "not granting ... due to not meeting element requirements"),
//! a grant whose target's own requirements fail is not applied: an "Additional Feat" item holding
//! an epic boon gives nothing below level 19. The equip command says so, and names an item of the
//! content whose grant lifts the requirement (AuroraLegacy's "Feat Unlocker"), so the user knows
//! why the feat is missing and what Aurora's content offers to ignore the prerequisite, and
//! points at Borealis's own "Override requirements" on the item (U12).

use crate::engine::Engine;
use crate::eval::requirement_ids;
use crate::state::{CharacterState, EquipSlot, InventoryItem};
use crate::universe::UElem;
use borealis_content::Rule;
use std::collections::HashSet;

/// One warning per element the item in line `uid` grants that `after` withholds because the
/// element's own requirements fail (the snapshot's `withheld_grants`; none while the line has
/// "Override requirements" on, U12).
pub(super) fn withheld_grant_warnings(
    engine: &Engine,
    after: &CharacterState,
    uid: &str,
) -> Vec<String> {
    let snap = engine.derive_with(after, false);
    let Some(line) = snap.inventory.items.iter().find(|i| i.uid == uid) else {
        return Vec::new();
    };
    let uni = engine.universe(after.ruleset);
    line.withheld_grants
        .iter()
        .filter_map(|g| uni.get(&g.id))
        .map(|t| message(engine, after, t))
        .collect()
}

fn message(engine: &Engine, state: &CharacterState, target: &UElem) -> String {
    let why = if target.el.prerequisite.trim().is_empty() {
        "its requirements are not met".to_string()
    } else {
        format!(
            "its prerequisite ({}) is not met",
            target.el.prerequisite.trim()
        )
    };
    let way = match unlocking_item(engine, state, target) {
        Some(unlocker) => {
            format!("Equip {unlocker}, or turn on Override requirements for this item.")
        }
        None => "Turn on Override requirements for this item to apply it anyway.".to_string(),
    };
    format!("{} is not applied yet: {why}. {way}", target.el.name)
}

/// The name of an item (not yet carried) whose grants include an id the target's requirements
/// name and which, equipped, lets the target apply.
fn unlocking_item(engine: &Engine, state: &CharacterState, target: &UElem) -> Option<String> {
    let uni = engine.universe(state.ruleset);
    let ids = requirement_ids(&target.req);
    let carried: HashSet<&str> = state
        .inventory
        .items
        .iter()
        .map(|i| i.item.as_str())
        .collect();
    uni.by_type("Item")
        .filter(|it| !carried.contains(it.el.id.as_str()))
        .filter(|it| {
            it.el
                .rules
                .iter()
                .any(|r| matches!(r, Rule::Grant(g) if ids.contains(g.id.as_str())))
        })
        .find(|it| {
            let mut trial = state.clone();
            let mut line = InventoryItem::new("withheld-trial".to_string(), it.el.id.clone());
            line.equipped = Some(EquipSlot::Worn);
            trial.inventory.items.push(line);
            engine
                .derive_with(&trial, false)
                .elements
                .iter()
                .any(|e| e.element.id == target.el.id)
        })
        .map(|it| it.el.name.clone())
}
