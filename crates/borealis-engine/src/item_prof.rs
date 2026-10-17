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
