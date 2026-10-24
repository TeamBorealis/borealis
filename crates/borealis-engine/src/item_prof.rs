//! "You are proficient with this item" (Elven Chain, Claws of the Umber Hulk, Warrior's Passkey):
//! setter `proficient` = `true` on the item (or on the magic item layered on a base item). The
//! character counts as proficient with that inventory line, whatever proficiencies it has; an
//! item that requires attunement grants it only while attuned.

use crate::graph::Graph;
use crate::inventory::item_facts;
use crate::universe::UElem;

/// Setter name.
pub const SETTER: &str = "proficient";

fn flagged(e: &UElem) -> bool {
    e.el.setter_value(SETTER)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

/// Is inventory line `uid` (base element `base`, optional magic `adorner`) one the character is
/// proficient with through the item itself?
#[must_use]
pub fn self_proficient(
    graph: &Graph<'_>,
    uid: &str,
    base: &UElem,
    adorner: Option<&UElem>,
) -> bool {
    let attuned = || {
        graph
            .nodes
            .iter()
            .any(|n| n.alive && n.attuned && n.item_uid.as_deref() == Some(uid))
    };
    [Some(base), adorner]
        .into_iter()
        .flatten()
        .filter(|e| flagged(e))
        .any(|e| !item_facts(e).requires_attunement || attuned())
}

/// Rule kind: proficiency with every weapon of a category that has one of the listed properties
/// (2024 Rogue: "Martial weapons that have the Finesse or Light property"; Monk: Light).
pub const FILTER_KIND: &str = "proficiency_filter";

/// Support tag of a weapon property (`Finesse` -> `ID_INTERNAL_WEAPON_PROPERTY_FINESSE`).
fn property_tag(name: &str) -> String {
    format!(
        "ID_INTERNAL_WEAPON_PROPERTY_{}",
        crate::universe::sanitize_id(name.trim()).replace('_', "")
    )
}

/// Does weapon `e` fall under a `proficiency_filter` rule (`weapon_category` Simple/Martial,
/// optional `properties_any`)?
fn filter_matches(filter: &crate::rule_view::Live<'_>, e: &UElem) -> bool {
    let has = |tag: &str| e.supports.iter().any(|s| s == tag);
    let category_ok = filter.str("weapon_category").is_none_or(|c| {
        let c = crate::universe::sanitize_id(c.trim());
        ["MELEE", "RANGED"]
            .iter()
            .any(|k| has(&format!("ID_INTERNAL_WEAPON_CATEGORY_{c}_{k}")))
    });
    let props = filter.list("properties_any");
    category_ok && (props.is_empty() || props.iter().any(|p| has(&property_tag(p))))
}

/// Is the character proficient with weapon `e` through a live `proficiency_filter` rule?
#[must_use]
pub fn filter_proficient(graph: &Graph<'_>, e: &UElem) -> bool {
    item_facts(e).weapon
        && crate::rule_view::live_rules(graph, &[FILTER_KIND])
            .iter()
            .any(|f| filter_matches(f, e))
}

/// Is the character proficient with weapon `e`: its proficiency element is held, or a live
/// `proficiency_filter` rule covers it?
#[must_use]
pub fn weapon_proficient(graph: &Graph<'_>, e: &UElem) -> bool {
    e.el.setter_value("proficiency")
        .is_some_and(|p| graph.has(p))
        || filter_proficient(graph, e)
}
