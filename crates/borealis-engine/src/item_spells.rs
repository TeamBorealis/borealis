//! Spells an equipped item grants without naming a spellcasting class.
//!
//! A class-less spell grant (a `grant_spell` with no `spell_list`, a `Spell` grant with no
//! `spellcasting`) registers the spell but belongs to no spellcasting block, so the sheet's
//! spell list never shows it. For spells that come from an item, one block per item (named
//! after it) lists them: the cantrip a worn circlet teaches, the spell a staff casts. Its
//! ability is the grant's `ability` (when it names one; the save DC and attack bonus follow
//! from it), and it has no slots. Spells granted by anything else, Aurora's proxy items
//! included, keep their place.

use crate::custom_rules::X_SPELL_ABILITY;
use crate::graph::{Graph, Node, NodeId, Origin};
use crate::snapshot::{SpellEntry, SpellcastingSnapshot};
use crate::stats::StatSet;
use borealis_content::Rule;

/// The item root a node hangs under, if any.
fn item_root(graph: &Graph<'_>, node: &Node<'_>) -> Option<NodeId> {
    std::iter::successors(node.parent, |id| graph.nodes[*id].parent)
        .find(|id| graph.nodes[*id].item_uid.is_some())
}

/// The one ability the grant of `node` names (`ability` of a `grant_spell`), as a full name.
fn grant_ability(graph: &Graph<'_>, node: &Node<'_>) -> Option<String> {
    let Origin::Granted(ri) = node.origin else {
        return None;
    };
    let parent = &graph.nodes[node.parent?];
    let Some(Rule::Grant(g)) = parent.e.el.rules.get(ri) else {
        return None;
    };
    let raw = g.extra.attributes.get(X_SPELL_ABILITY)?;
    let mut named = raw
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(crate::ids::ability_index);
    let first = named.next()?;
    // a choice between abilities names no single one
    named
        .all(|i| i == first)
        .then(|| crate::ids::ABILITIES[first].to_string())
}

pub(crate) fn entry(graph: &Graph<'_>, node: &Node<'_>) -> SpellEntry {
    SpellEntry {
        id: node.e.el.id.clone(),
        name: node.e.el.name.clone(),
        level: crate::spells::spell_level(node.e),
        prepared: true,
        always_prepared: true,
        known: true,
        damage: crate::spells::cantrip_damage(node.e, graph.total_level()),
        granted_by: crate::spells::spell_grant(graph, node),
    }
}

pub(crate) fn block(
    name: String,
    ability: Option<String>,
    stats: &StatSet,
) -> SpellcastingSnapshot {
    let (dc, attack) = ability.as_deref().map_or((0, 0), |a| {
        let m = stats.sum(&format!("{a}:modifier")) + stats.sum("proficiency");
        (8 + m, m)
    });
    SpellcastingSnapshot {
        name,
        ability: ability
            .map(|a| crate::rule_view::cap(&a))
            .unwrap_or_default(),
        prepare: false,
        allow_replace: false,
        dc,
        attack,
        slots: [0; 9],
        pact: None,
        prepare_limit: None,
        prepared_count: 0,
        spells: Vec::new(),
        prepare_from_list: false,
        prepare_source: String::new(),
        spellbook: None,
        arcanum: Vec::new(),
        prepared_change: None,
        ritual_source: None,
        extended_lists: Vec::new(),
    }
}

/// Append one block per item for the class-less spells it grants (see the module docs).
pub fn add_item_blocks(graph: &Graph<'_>, stats: &StatSet, out: &mut Vec<SpellcastingSnapshot>) {
    for (_, n) in graph
        .nodes_of_type("Spell")
        .filter(|(_, n)| n.spell_class.is_none())
    {
        let Some(root) = item_root(graph, n) else {
            continue;
        };
        // Aurora's proxy items ("Additional Spell, X", `crate::proxy`) add a spell outside any
        // block: it stays with the character's additional spells, as in Aurora.
        if graph.nodes[root]
            .e
            .el
            .setter_value("slot")
            .is_some_and(|s| s.eq_ignore_ascii_case("proxy"))
        {
            continue;
        }
        let name = graph.nodes[root].e.el.name.clone();
        let at = match out.iter().position(|b| b.name == name) {
            Some(i) => i,
            None => {
                out.push(block(name, grant_ability(graph, n), stats));
                out.len() - 1
            }
        };
        let spells = &mut out[at].spells;
        if !spells.iter().any(|s| s.id == n.e.el.id) {
            spells.push(entry(graph, n));
            spells.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.name.cmp(&b.name)));
        }
    }
}
