//! Warnings for grants an equipped item cannot apply yet (Discord #3, GitHub issue 3).
//!
//! Like Aurora (`ProgressionManager`: "not granting ... due to not meeting element requirements"),
//! a grant whose target's own requirements fail is not applied: an "Additional Feat" item holding
//! an epic boon gives nothing below level 19. The equip command says so, and names an item of the
//! content whose grant lifts the requirement (AuroraLegacy's "Feat Unlocker"), so the user knows
//! why the feat is missing and what Aurora's content offers to ignore the prerequisite.

use crate::engine::Engine;
use crate::eval::requirement_ids;
use crate::state::{CharacterState, EquipSlot, InventoryItem};
use crate::universe::UElem;
use borealis_content::Rule;
use std::collections::HashSet;

/// One warning per grant of the item in line `uid` whose target `after` does not hold although
/// the target exists and has requirements of its own (the grant rule's own conditions hold).
pub(super) fn withheld_grant_warnings(
    engine: &Engine,
    after: &CharacterState,
    uid: &str,
) -> Vec<String> {
    let uni = engine.universe(after.ruleset);
    let Some(line) = after.inventory.items.iter().find(|i| i.uid == uid) else {
        return Vec::new();
    };
    let Some(item) = uni.get(&line.item) else {
        return Vec::new();
    };
    // (target, the grant's own level): a grant gated by its own requirements is not about the
    // target, so it is left out
    let targets: Vec<(&UElem, u32)> = item
        .el
        .rules
        .iter()
        .filter_map(|r| match r {
            Rule::Grant(g) if g.requirements.is_none() => {
                uni.get(&g.id).map(|t| (t, g.level.unwrap_or(1)))
            }
            _ => None,
        })
        .filter(|(t, _)| !t.el.requirements.trim().is_empty())
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }
    let snap = engine.derive_with(after, false);
    let held: HashSet<&str> = snap
        .elements
        .iter()
        .map(|e| e.element.id.as_str())
        .collect();
    targets
        .into_iter()
        // a grant waiting on a higher character level is waiting on that, not on the target
        .filter(|(t, level)| *level <= snap.level.max(1) && !held.contains(t.el.id.as_str()))
        .map(|(t, _)| message(engine, after, t))
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
    let mut text = format!("{} is not applied yet: {why}.", target.el.name);
    if let Some(unlocker) = unlocking_item(engine, state, target) {
        text.push_str(&format!(" Equipping {unlocker} lifts it."));
    }
    text
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
