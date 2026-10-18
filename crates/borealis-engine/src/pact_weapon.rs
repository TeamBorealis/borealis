//! 2024 Pact of the Blade (`pact_weapon` rule, SRD 5.2.1 Warlock Invocations): the warlock bonds
//! with a weapon of the listed kinds (`weapon_kinds`, e.g. `Simple Melee`, `Martial Melee`);
//! until the bond ends they are proficient with it and may use the rule's `ability` (Charisma)
//! for its attack and damage rolls instead of Strength or Dexterity.
//!
//! The character keeps no bond record yet, so the bonded weapon is the first carried weapon of
//! an allowed kind in inventory order (one weapon at a time, as the rule allows); its attack
//! line uses the better of its own ability and the pact ability.

use crate::graph::Graph;
use crate::state::CharacterState;
use crate::universe::{UElem, Universe};

/// Rule kind.
pub const KIND: &str = "pact_weapon";

fn covers(kinds: &[String], e: &UElem) -> bool {
    kinds.is_empty()
        || kinds.iter().any(|k| {
            let tag = format!(
                "ID_INTERNAL_WEAPON_CATEGORY_{}",
                crate::universe::sanitize_id(k.trim())
            );
            e.supports.contains(&tag)
        })
}

/// The bonded weapon line (inventory uid) and the ability index (0 = Strength .. 5 =
/// Charisma) it may use, when a live `pact_weapon` rule applies.
#[must_use]
pub fn bond(uni: &Universe, graph: &Graph<'_>, state: &CharacterState) -> Option<(String, usize)> {
    let rules = crate::rule_view::live_rules(graph, &[KIND]);
    let rule = rules.first()?;
    let ability = crate::ids::ability_index(rule.str("ability").unwrap_or("charisma"))?;
    let kinds = rule.list("weapon_kinds");
    state.inventory.items.iter().find_map(|it| {
        let (e, _) = crate::item_base::resolve(uni, it)?;
        (crate::inventory::item_facts(e).weapon && covers(&kinds, e))
            .then(|| (it.uid.clone(), ability))
    })
}
