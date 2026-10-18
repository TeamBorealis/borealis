//! `damage_bonus_spell` (Elemental Affinity) on spell damage rows.
//!
//! SRD 5.1 Draconic Bloodline: "when you cast a spell that deals damage of the type associated
//! with your draconic ancestry, you can add your Charisma modifier to one damage roll of that
//! spell". SRD 5.2.1 Draconic Sorcery: the same for the damage type chosen with the feature.
//! Aurora prints this as sheet text only (PHB class-sorcerer.xml), so the rule follows the SRD.
//! The rule's damage type comes from one of:
//! - `damage_type_stat`: an inline statistic holding the type (`draconic-ancestor:damage type`
//!   = `Fire`, set by the chosen Dragon Ancestor);
//! - `damage_type_param`: the select of the rule's own element with that name; the chosen
//!   option's `data.damage_type` is the type (`Elemental Affinity Damage Type` -> Fire).
//!
//! A cantrip deals one damage roll, so its row adds `value` when the types match.

use crate::graph::{Graph, NodeId, Origin};
use crate::rule_view::Live;
use crate::stats::StatSet;
use borealis_content::Rule;
use serde_json::Value;

const KIND: &str = "damage_bonus_spell";

/// Damage type of the option chosen in the select named `select` of `node`.
fn chosen_type(graph: &Graph<'_>, node: NodeId, select: &str) -> Option<String> {
    let n = &graph.nodes[node];
    n.children.iter().find_map(|&c| {
        let child = &graph.nodes[c];
        let Origin::Selected(si) = child.origin else {
            return None;
        };
        let slot = n.slots.get(si)?;
        let named = matches!(&n.e.el.rules[slot.rule],
            Rule::Select(r) if r.name.trim().eq_ignore_ascii_case(select.trim()));
        if !(child.alive && named) {
            return None;
        }
        child
            .e
            .el
            .data
            .get("damage_type")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

/// The damage type a live rule applies to, if it names one that is set.
fn rule_type(graph: &Graph<'_>, l: &Live<'_>) -> Option<String> {
    if let Some(stat) = l.str("damage_type_stat") {
        return crate::breath::inline_text(graph, stat);
    }
    chosen_type(graph, l.node, l.str("damage_type_param")?)
}

/// Bonus to one damage roll of a spell dealing `damage_type` damage.
#[must_use]
pub fn bonus(graph: &Graph<'_>, stats: &StatSet, mods: &[i32; 6], damage_type: &str) -> i32 {
    let ty = damage_type.trim();
    if ty.is_empty() {
        return 0;
    }
    crate::rule_view::live_rules(graph, &[KIND])
        .iter()
        .filter(|l| rule_type(graph, l).is_some_and(|t| t.trim().eq_ignore_ascii_case(ty)))
        .filter_map(|l| crate::play_rules::eval_formula(graph, stats, mods, l.str("value")?))
        .sum()
}
