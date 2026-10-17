//! Commands of slice D: learned Wild Shape forms and starting-equipment choices.

use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::packages::add_coins;
use crate::state::{CharacterState, InventoryItem};
use std::collections::BTreeSet;

/// Replace the set of learned Wild Shape forms.
///
/// # Errors
/// [`CommandError::Level`] when the character has no Wild Shape or too many forms are named,
/// [`CommandError::NotAllowed`] for a creature that does not fit the current limits.
pub fn set_wild_shape_forms(engine: &Engine, state: &CharacterState, ids: &[String]) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let Some(ws) = snap.wild_shape else {
        return Err(CommandError::Level(
            "the character has no Wild Shape".into(),
        ));
    };
    let forms: BTreeSet<String> = ids.iter().cloned().collect();
    if forms.len() > ws.max_forms as usize {
        return Err(CommandError::Level(format!(
            "Wild Shape knows {} forms at this level, {} were named",
            ws.max_forms,
            forms.len()
        )));
    }
    if let Some(bad) = forms
        .iter()
        .find(|f| !ws.options.iter().any(|o| &o.id == *f))
    {
        return Err(CommandError::NotAllowed(bad.clone()));
    }
    let mut next = state.clone();
    next.play.wild_shape_forms = forms;
    Ok(CommandOutcome::new(next))
}

fn next_item_uid(state: &CharacterState) -> u32 {
    state
        .inventory
        .items
        .iter()
        .filter_map(|i| {
            i.uid
                .strip_prefix("item-")
                .and_then(|n| n.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1
}

/// Take a starting-equipment option: its items join the inventory and its coins the purse.
///
/// # Errors
/// [`CommandError::UnknownElement`] for a package the character is not offered,
/// [`CommandError::NotAllowed`] for an unknown option label, [`CommandError::Inventory`] when
/// the package was already taken.
pub fn choose_equipment_package(
    engine: &Engine,
    state: &CharacterState,
    package: &str,
    option: &str,
) -> CmdResult {
    let snap = engine.derive_with(state, false);
    let pkg = snap
        .equipment_packages
        .iter()
        .find(|p| p.package == package)
        .ok_or_else(|| CommandError::UnknownElement(package.to_string()))?;
    if let Some(done) = &pkg.chosen {
        return Err(CommandError::Inventory(format!(
            "{} was already taken as option {done}",
            pkg.name
        )));
    }
    let opt = pkg
        .options
        .iter()
        .find(|o| o.id == option)
        .ok_or_else(|| CommandError::NotAllowed(option.to_string()))?;
    let mut next = state.clone();
    let mut out = CommandOutcome::new(state.clone());
    let mut uid = next_item_uid(state);
    for it in &opt.items {
        let Some(id) = it
            .id
            .as_deref()
            .filter(|i| engine.has_element(state.ruleset, i))
        else {
            out.warnings.push(format!(
                "{} is not in the installed content and was skipped",
                it.name
            ));
            continue;
        };
        let mut item = InventoryItem::new(format!("item-{uid}"), id);
        uid += 1;
        item.amount = it.qty.max(1);
        item.acquired_from = Some(pkg.name.clone());
        next.inventory.items.push(crate::rolled::with_rolls(
            engine.universe(state.ruleset),
            &item,
        ));
    }
    next.inventory.coins = add_coins(&next, opt.coins);
    next.play
        .equipment_choices
        .insert(package.to_string(), option.to_string());
    out.state = next;
    Ok(out)
}
