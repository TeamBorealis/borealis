//! Inventory, equipment, attunement and money commands.

use super::selection::prune;
use super::{CmdResult, CommandError, CommandOutcome};
use crate::engine::Engine;
use crate::inventory::{item_facts, ItemFacts};
use crate::money::Coin;
use crate::state::{CharacterState, EquipSlot, InventoryItem};
use std::collections::HashMap;

fn index_of(state: &CharacterState, uid: &str) -> Result<usize, CommandError> {
    state
        .inventory
        .items
        .iter()
        .position(|i| i.uid == uid)
        .ok_or_else(|| CommandError::Inventory(format!("no item {uid}")))
}

fn next_uid(state: &CharacterState) -> String {
    let n = state
        .inventory
        .items
        .iter()
        .filter_map(|i| {
            i.uid
                .strip_prefix("item-")
                .and_then(|n| n.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0);
    format!("item-{}", n + 1)
}

/// Add an item with a generated uid.
///
/// # Errors
/// [`CommandError::Inventory`] if the item is not installed.
pub fn add_item(engine: &Engine, state: &CharacterState, item: &str, amount: u32) -> CmdResult {
    if !engine.has_element(state.ruleset, item) {
        return Err(CommandError::Inventory(format!("unknown item {item}")));
    }
    let mut next = state.clone();
    let mut it = InventoryItem::new(next_uid(state), item);
    it.amount = amount.max(1);
    next.inventory.items.push(it);
    Ok(CommandOutcome::new(next))
}

/// Remove an item (and prune selections its rules opened).
///
/// # Errors
/// [`CommandError::Inventory`] for unknown uids.
pub fn remove_item(engine: &Engine, state: &CharacterState, uid: &str) -> CmdResult {
    let i = index_of(state, uid)?;
    let mut next = state.clone();
    next.inventory.items.remove(i);
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Apply a magic item (adorner) on top of a base item.
///
/// # Errors
/// [`CommandError::Inventory`] for unknown uids or adorners.
pub fn set_adorner(
    engine: &Engine,
    state: &CharacterState,
    uid: &str,
    adorner: Option<&str>,
) -> CmdResult {
    let i = index_of(state, uid)?;
    if let Some(a) = adorner {
        if !engine.has_element(state.ruleset, a) {
            return Err(CommandError::Inventory(format!("unknown item {a}")));
        }
    }
    let mut next = state.clone();
    next.inventory.items[i].adorner = adorner.map(String::from);
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Equip an item, handling hands and armor. An attunable item is attuned when the limit allows;
/// otherwise it is equipped unattuned and a warning is returned.
///
/// # Errors
/// [`CommandError::Inventory`] for unknown uids or items without an equip slot.
pub fn equip(
    engine: &Engine,
    state: &CharacterState,
    uid: &str,
    requested: Option<EquipSlot>,
) -> CmdResult {
    let i = index_of(state, uid)?;
    let (facts_of, adorner_requires, limit) = engine.with_evaluation(state, |ev| {
        let facts: HashMap<String, ItemFacts> = state
            .inventory
            .items
            .iter()
            .filter_map(|it| {
                ev.uni
                    .get(&it.item)
                    .map(|e| (it.uid.clone(), crate::item_base::effective_facts(ev.uni, e)))
            })
            .collect();
        let adorner = state.inventory.items[i]
            .adorner
            .as_deref()
            .and_then(|a| ev.uni.get(a))
            .is_some_and(|a| item_facts(a).requires_attunement);
        (facts, adorner, ev.limit)
    });
    let facts = facts_of
        .get(uid)
        .cloned()
        .ok_or_else(|| CommandError::Inventory("the item is not installed".into()))?;
    if !facts.equippable {
        return Err(CommandError::Inventory(
            "this item cannot be equipped".into(),
        ));
    }
    let primary_taken = state
        .inventory
        .items
        .iter()
        .any(|x| x.equipped == Some(EquipSlot::PrimaryHand) && x.uid != uid);
    let slot = requested.unwrap_or(if facts.body_armor {
        EquipSlot::Armor
    } else if facts.shield {
        EquipSlot::SecondaryHand
    } else if facts.weapon && (!primary_taken || facts.two_handed) {
        EquipSlot::PrimaryHand
    } else if facts.weapon {
        EquipSlot::SecondaryHand
    } else {
        EquipSlot::Worn
    });
    let mut next = state.clone();
    if slot != EquipSlot::Worn {
        for it in &mut next.inventory.items {
            if it.uid != uid && it.equipped == Some(slot) {
                clear_equipped(it);
            }
        }
    }
    let holds_both = facts.two_handed || next.inventory.items[i].versatile;
    if slot == EquipSlot::PrimaryHand && holds_both {
        for it in &mut next.inventory.items {
            if it.uid != uid && it.equipped == Some(EquipSlot::SecondaryHand) {
                clear_equipped(it);
            }
        }
    }
    if slot == EquipSlot::SecondaryHand {
        for it in &mut next.inventory.items {
            let primary_two_handed = facts_of.get(&it.uid).is_some_and(|f| f.two_handed);
            if it.uid != uid && it.equipped == Some(EquipSlot::PrimaryHand) {
                if primary_two_handed {
                    clear_equipped(it);
                } else {
                    it.versatile = false;
                }
            }
        }
        next.inventory.items[i].versatile = false;
    }
    next.inventory.items[i].equipped = Some(slot);
    next.inventory.items[i].attuned = false;
    let mut warnings = Vec::new();
    if facts.requires_attunement || adorner_requires {
        let inv = engine.derive_with(&next, false).inventory;
        let used = inv.attuned;
        let restricted = inv
            .items
            .iter()
            .find(|l| l.uid == uid && l.attunement_barred);
        if let Some(line) = restricted {
            warnings.push(format!(
                "{}: the item is equipped but not attuned",
                barred_message(line)
            ));
        } else if used < limit {
            next.inventory.items[i].attuned = true;
        } else {
            warnings.push(format!(
                "attunement limit of {limit} reached: the item is equipped but not attuned"
            ));
        }
    }
    let mut out = CommandOutcome::new(prune(engine, &next));
    out.warnings = warnings;
    Ok(out)
}

/// "Moonblade requires attunement by an elf or half-elf of neutral good alignment".
fn barred_message(line: &crate::snapshot::ItemSnapshot) -> String {
    format!(
        "{} requires attunement {}",
        line.name,
        line.attunement_restriction
            .as_deref()
            .unwrap_or("by someone else")
    )
}

fn clear_equipped(it: &mut InventoryItem) {
    it.equipped = None;
    it.attuned = false;
    it.versatile = false;
}

/// Unequip an item (this also ends attunement).
///
/// # Errors
/// [`CommandError::Inventory`] for unknown uids.
pub fn unequip(engine: &Engine, state: &CharacterState, uid: &str) -> CmdResult {
    let i = index_of(state, uid)?;
    let mut next = state.clone();
    let it = &mut next.inventory.items[i];
    it.equipped = None;
    it.attuned = false;
    it.versatile = false;
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Attune to an equipped item that requires it.
///
/// # Errors
/// [`CommandError::Inventory`] if not equipped, not attunable, or the limit is reached.
pub fn attune(engine: &Engine, state: &CharacterState, uid: &str) -> CmdResult {
    let i = index_of(state, uid)?;
    if state.inventory.items[i].equipped.is_none() {
        return Err(CommandError::Inventory("equip the item first".into()));
    }
    let snap = engine.derive_with(state, false);
    let line = snap
        .inventory
        .items
        .iter()
        .find(|l| l.uid == uid)
        .ok_or_else(|| CommandError::Inventory("unknown item".into()))?;
    if !line.requires_attunement {
        return Err(CommandError::Inventory(
            "this item does not require attunement".into(),
        ));
    }
    if line.attunement_barred && !line.attuned {
        return Err(CommandError::Inventory(barred_message(line)));
    }
    if snap.inventory.attuned >= snap.inventory.attunement_limit && !line.attuned {
        return Err(CommandError::Inventory(format!(
            "attunement limit of {} reached",
            snap.inventory.attunement_limit
        )));
    }
    let mut next = state.clone();
    next.inventory.items[i].attuned = true;
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// End attunement without unequipping.
///
/// # Errors
/// [`CommandError::Inventory`] for unknown uids.
pub fn unattune(engine: &Engine, state: &CharacterState, uid: &str) -> CmdResult {
    let i = index_of(state, uid)?;
    let mut next = state.clone();
    next.inventory.items[i].attuned = false;
    Ok(CommandOutcome::new(prune(engine, &next)))
}

/// Move an item to a storage container (`None` = carried).
///
/// # Errors
/// [`CommandError::Inventory`] for unknown uids or containers.
pub fn move_to_storage(state: &CharacterState, uid: &str, storage: Option<&str>) -> CmdResult {
    let i = index_of(state, uid)?;
    if let Some(s) = storage {
        if !state.inventory.storages.iter().any(|x| x == s) {
            return Err(CommandError::Inventory(format!("no storage called {s}")));
        }
    }
    let mut next = state.clone();
    let it = &mut next.inventory.items[i];
    it.storage = storage.map(String::from);
    if it.storage.is_some() {
        it.equipped = None;
        it.attuned = false;
    }
    Ok(CommandOutcome::new(next))
}

/// Add coins.
#[must_use]
pub fn deposit(state: &CharacterState, coin: Coin, n: u64) -> CharacterState {
    let mut next = state.clone();
    next.inventory.coins = state.inventory.coins.deposit(coin, n);
    next
}

/// Remove coins with make-change.
///
/// # Errors
/// [`CommandError::Money`] when the total value is insufficient.
pub fn withdraw(state: &CharacterState, coin: Coin, n: u64) -> CmdResult {
    let coins = state
        .inventory
        .coins
        .withdraw(coin, n)
        .ok_or_else(|| CommandError::Money("not enough money".into()))?;
    let mut next = state.clone();
    next.inventory.coins = coins;
    Ok(CommandOutcome::new(next))
}
