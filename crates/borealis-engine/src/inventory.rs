//! Inventory facts: which items take part in the graph, equipped state for conditions,
//! attunement, weight and carry capacity.

use crate::diag::{code, DiagSink, Diagnostic, Severity};
use crate::eval::{EquipState, HeldItem};
use crate::graph::ItemRoot;
use crate::ids;
use crate::state::{EquipSlot, Inventory, InventoryItem};
use crate::universe::{UElem, Universe};

/// Derived facts about an item element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemFacts {
    /// Setter `attunement` is set and not false.
    pub requires_attunement: bool,
    /// Type `Armor` and not a shield.
    pub body_armor: bool,
    /// Counts as a shield.
    pub shield: bool,
    /// Type `Weapon`.
    pub weapon: bool,
    /// Occupies both hands (`slot` = twohand).
    pub two_handed: bool,
    /// Has versatile damage.
    pub versatile: bool,
    /// Has an equip slot.
    pub equippable: bool,
}

/// Compute [`ItemFacts`].
#[must_use]
pub fn item_facts(e: &UElem) -> ItemFacts {
    let slot = e.el.setter_value("slot").unwrap_or("").to_lowercase();
    let armor = e.el.setter_value("armor").unwrap_or("").to_lowercase();
    let shield = armor == "shield" || e.supports.iter().any(|s| s == ids::SHIELD_GROUP);
    let att =
        e.el.setter_value("attunement")
            .map(|v| v.trim().to_lowercase())
            .is_some_and(|v| !v.is_empty() && v != "false" && v != "0");
    ItemFacts {
        requires_attunement: att,
        body_armor: e.el.ty == "Armor" && !shield,
        shield,
        weapon: e.el.ty == "Weapon",
        two_handed: slot == "twohand",
        versatile: e
            .el
            .setter_value("versatile")
            .is_some_and(|v| !v.trim().is_empty()),
        equippable: !slot.is_empty() && slot != "proxy",
    }
}

fn held(e: &UElem, f: &ItemFacts) -> HeldItem {
    HeldItem {
        id: e.el.id.to_lowercase(),
        name: e.el.name.to_lowercase(),
        armor_kind: if f.shield {
            "shield".into()
        } else {
            e.el.setter_value("armor").unwrap_or("").to_lowercase()
        },
        weapon: f.weapon,
        versatile_damage: f.versatile,
        shield: f.shield,
    }
}

/// Result of resolving the inventory against content.
pub struct ActiveItems<'u> {
    /// Roots to register in the graph.
    pub roots: Vec<ItemRoot<'u>>,
    /// Facts for equipped conditions.
    pub equip: EquipState,
    /// Attuned items counted (after the limit is applied).
    pub attuned_count: u32,
}

/// Decide which items are registered. Attunement beyond `limit` is ignored in inventory order
/// (the item stays equipped but unattuned).
#[must_use]
pub fn active_items<'u>(
    uni: &'u Universe,
    inv: &Inventory,
    limit: u32,
    diags: &mut DiagSink,
) -> ActiveItems<'u> {
    let mut roots = Vec::new();
    let mut eq = EquipState::default();
    let mut attuned_count = 0u32;
    for it in &inv.items {
        let Some(base) = uni.get(&it.item) else {
            diags.push(
                Diagnostic::new(
                    code::CONTENT_MISSING_ELEMENT,
                    Severity::Warning,
                    format!("inventory item {} is not installed", it.item),
                )
                .at(it.item.clone()),
            );
            continue;
        };
        let Some(slot) = it.equipped else { continue };
        let bf = item_facts(base);
        let adorner = it.adorner.as_deref().and_then(|a| uni.get(a));
        let af = adorner.map(item_facts);
        let needs_att =
            bf.requires_attunement || af.as_ref().is_some_and(|f| f.requires_attunement);
        let mut attuned = it.attuned && needs_att;
        if attuned {
            if attuned_count >= limit {
                attuned = false;
                diags.push(
                    Diagnostic::new(
                        code::ATTUNEMENT_OVER_LIMIT,
                        Severity::Warning,
                        format!(
                            "{} is equipped but not attuned: the limit of {limit} is reached",
                            base.el.name
                        ),
                    )
                    .at(base.el.id.clone()),
                );
            } else {
                attuned_count += 1;
            }
        }
        let base_registered =
            bf.body_armor || bf.weapon || bf.shield || !bf.requires_attunement || attuned;
        let adorner_registered =
            adorner.is_some() && (!af.as_ref().is_some_and(|f| f.requires_attunement) || attuned);
        if base_registered && bf.equippable {
            roots.push(ItemRoot {
                e: base,
                uid: it.uid.clone(),
                attuned,
            });
        }
        if let (Some(a), true) = (adorner, adorner_registered) {
            roots.push(ItemRoot {
                e: a,
                uid: it.uid.clone(),
                attuned,
            });
        }
        if !(base_registered || adorner_registered) {
            continue;
        }
        let h = held(base, &bf);
        match slot {
            EquipSlot::Armor => eq.armor = Some(h),
            EquipSlot::PrimaryHand => {
                let both = bf.two_handed || it.versatile;
                eq.primary = Some(h.clone());
                if both {
                    eq.secondary = Some(h);
                    eq.both_hands = true;
                }
            }
            EquipSlot::SecondaryHand => eq.secondary = Some(h),
            EquipSlot::Worn => {}
        }
        for e in std::iter::once(base).chain(adorner) {
            eq.equipped.insert(e.el.id.to_lowercase());
            if attuned {
                eq.attuned.insert(e.el.id.to_lowercase());
            }
        }
    }
    ActiveItems {
        roots,
        equip: eq,
        attuned_count,
    }
}

/// Price of an item in gold pieces (setter `cost`, attribute `currency`; decimal).
#[must_use]
pub fn item_cost_gp(e: &UElem) -> f64 {
    let Some(c) = e.el.setter("cost") else {
        return 0.0;
    };
    let v = c
        .value
        .trim()
        .replace(',', "")
        .parse::<f64>()
        .unwrap_or(0.0);
    let rate = match c.attribute("currency").map(str::to_lowercase).as_deref() {
        Some("cp") => 0.01,
        Some("sp") => 0.1,
        Some("ep") => 0.5,
        Some("pp") => 10.0,
        _ => 1.0,
    };
    v * rate
}

/// Weight of one item in pounds (setter `weight`, attribute `lb`; decimal).
#[must_use]
pub fn item_weight(uni: &Universe, it: &InventoryItem) -> f64 {
    let lb = |e: &UElem| -> Option<f64> {
        let s = e.el.setter("weight")?;
        if s.attribute("excludeEncumbrance")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
        {
            return Some(0.0);
        }
        s.attribute("lb").and_then(|v| v.trim().parse::<f64>().ok())
    };
    let base = uni.get(&it.item).and_then(lb).unwrap_or(0.0);
    let w = it
        .adorner
        .as_deref()
        .and_then(|a| uni.get(a))
        .and_then(lb)
        .filter(|w| *w > 0.0)
        .unwrap_or(base);
    w * f64::from(it.amount)
}

/// Total carried weight: items outside storage plus coins (1 lb per 50 coins).
#[must_use]
pub fn carried_weight(uni: &Universe, inv: &Inventory) -> f64 {
    let items: f64 = inv
        .items
        .iter()
        .filter(|i| i.storage.is_none())
        .map(|i| item_weight(uni, i))
        .sum();
    items + inv.coins.count() as f64 / 50.0
}

/// Size multiplier from the size element id and capacity grants.
#[must_use]
pub fn capacity_multiplier(
    size_id: Option<&str>,
    larger: bool,
    doubled: bool,
    halved: bool,
) -> f64 {
    let mut tier = match size_id {
        Some("ID_SIZE_TINY") => 0,
        Some("ID_SIZE_LARGE" | "ID_SIZE_HUGE" | "ID_SIZE_GARGANTUAN" | "ID_SIZE_COLOSSAL") => 2,
        _ => 1,
    };
    if larger {
        tier = (tier + 1).min(2);
    }
    let mut m = [7.5, 15.0, 30.0][tier];
    if doubled {
        m *= 2.0;
    }
    if halved {
        m /= 2.0;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_parse_as_decimals_e27() {
        let mut e = borealis_content::Element::new("ID_X", "X", "Item", "S");
        let mut c = borealis_content::Setter::new("cost", "0.5");
        c.attributes.insert("currency".into(), "sp".into());
        e.setters.push(c);
        let u = crate::universe::Universe::new(
            &borealis_content::ContentStoreBuilder::new()
                .add_file(
                    "t",
                    borealis_content::PackOrigin::Core,
                    borealis_content::ContentFile::from_json_str(
                        &serde_json::to_string(
                            &serde_json::json!({"format":"borealis-content/1","elements":[e]}),
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                )
                .build(),
            crate::profile::default_ruleset(),
        );
        let el = u.get("ID_X").unwrap();
        assert!((item_cost_gp(el) - 0.05).abs() < 1e-12);
    }

    #[test]
    fn tv23_carry_capacity() {
        let cap = |size: Option<&str>, l, d, h| 16.0 * capacity_multiplier(size, l, d, h);
        assert_eq!(cap(Some("ID_SIZE_MEDIUM"), false, false, false), 240.0);
        assert_eq!(cap(Some("ID_SIZE_SMALL"), false, false, false), 240.0);
        assert_eq!(cap(Some("ID_SIZE_HUGE"), false, false, false), 480.0);
        assert_eq!(cap(Some("ID_SIZE_TINY"), false, false, false), 120.0);
        assert_eq!(cap(Some("ID_SIZE_MEDIUM"), true, false, false), 480.0);
        assert_eq!(cap(Some("ID_SIZE_MEDIUM"), false, true, false), 480.0);
        assert_eq!(cap(Some("ID_SIZE_MEDIUM"), false, false, true), 120.0);
        assert_eq!(cap(Some("ID_SIZE_MEDIUM"), false, true, true), 240.0);
        assert_eq!(cap(None, false, false, false), 240.0);
        assert_eq!(cap(Some("ID_SIZE_TINY"), true, false, false), 240.0);
    }
}
