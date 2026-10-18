//! Spellcasting blocks of spells a character casts outside any class (st3-golden F5): 2024
//! species lineages and legacies (SRD 5.2.1 Elven Lineage, Gnomish Lineage, Fiendish Legacy)
//! and feats (Magic Initiate).
//!
//! The SRD 5.2.1 pack gives these no `spellcasting` block: a feat picks its spells in selects
//! tagged with a spellcasting name that no block carries (`Magic Initiate`), and a species
//! grants fixed spells (`grant_spell`) whose ability is a parameter chosen elsewhere in the
//! species (`{"param": "Lineage Spellcasting Ability"}`). Aurora's own 2024 content has no
//! block either (spells granted by a racial trait are additional spells), so these blocks follow
//! the SRD: "Intelligence, Wisdom, or Charisma is your spellcasting ability for the spells you
//! cast with this trait/feat". Each block has that ability's save DC (8 + modifier +
//! proficiency) and attack bonus, no slots, and lists its cantrips and always-prepared spells,
//! so cantrip attack rows (`spell_attacks`) follow from it.
//!
//! Blocks:
//! - a spellcasting name no block carries (`Magic Initiate`): one block per granting element
//!   instance, named `<name> (<Spell List>)` when the instance chose a spell list (a repeated
//!   feat keeps one block per list); its ability is the element's `ability` parameter.
//! - class-less spells granted with a parameter ability: one block per element that owns the
//!   parameter, named after the option chosen there (`Forest Gnome`, `Wood Elf`); a one-word
//!   option takes the owner's last word (`Infernal` of Fiendish Legacy -> `Infernal Legacy`,
//!   the SRD's "Infernal legacy"). A spell another trait grants with the same parameter
//!   (Tiefling Otherworldly Presence: Thaumaturgy) joins that block.

use crate::custom_rules::{param_key, X_SPELL_ABILITY};
use crate::graph::{Graph, Node, NodeId, Origin};
use crate::snapshot::SpellcastingSnapshot;
use crate::stats::StatSet;
use borealis_content::Rule;
use serde_json::Value;

/// The ability a rule field names: `"charisma"`, `{"param": "<name>"}` (the parameter's chosen
/// value, owned by `node` or found near it), or `{"choice": [one ability]}`. Returns the full
/// lower-case ability name and the node owning the parameter (if any).
fn ability_of(graph: &Graph<'_>, node: NodeId, v: &Value) -> Option<(String, Option<NodeId>)> {
    let named =
        |t: &str| crate::ids::ability_index(t).map(|i| crate::ids::ABILITIES[i].to_string());
    match v {
        Value::String(s) => {
            if let Ok(inner) = serde_json::from_str::<Value>(s) {
                if inner.is_object() {
                    return ability_of(graph, node, &inner);
                }
            }
            named(s).map(|a| (a, None))
        }
        Value::Object(m) => {
            if let Some(p) = m.get("param").and_then(Value::as_str) {
                let (owner, value) = find_param(graph, node, &param_key(p))?;
                return named(&value).map(|a| (a, Some(owner)));
            }
            match m.get("choice").and_then(Value::as_array).map(Vec::as_slice) {
                Some([one]) => one.as_str().and_then(named).map(|a| (a, None)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The nearest node holding a value for parameter `key`: `from` itself, else the first node in
/// the subtree of each ancestor in turn (a species parameter chosen by a sibling trait).
fn find_param(graph: &Graph<'_>, from: NodeId, key: &str) -> Option<(NodeId, String)> {
    let mut at = Some(from);
    while let Some(a) = at {
        let mut stack = vec![a];
        while let Some(n) = stack.pop() {
            let node = &graph.nodes[n];
            if !node.alive {
                continue;
            }
            if let Some(v) = graph.param_value(n, key) {
                return Some((n, v));
            }
            stack.extend(node.children.iter().rev());
        }
        at = graph.nodes[a].parent;
    }
    None
}

/// Block name of a parameter owner: the option chosen in one of its (non-parameter) selects,
/// else its own name (see the module docs).
fn owner_block_name(graph: &Graph<'_>, owner: NodeId) -> String {
    let n = &graph.nodes[owner];
    let option = n.children.iter().map(|c| &graph.nodes[*c]).find(|c| {
        c.alive
            && matches!(c.origin, Origin::Selected(si) if n.slots.get(si).is_some_and(|sl| {
                matches!(&n.e.el.rules[sl.rule], Rule::Select(r)
                    if crate::custom_rules::kind_of(r) != Some("param"))
            }))
    });
    match option {
        Some(o) if o.e.el.name.trim().contains(' ') => o.e.el.name.trim().to_string(),
        Some(o) => match n.e.el.name.trim().rsplit(' ').next() {
            Some(last) if n.e.el.name.trim().contains(' ') => {
                format!("{} {last}", o.e.el.name.trim())
            }
            _ => o.e.el.name.trim().to_string(),
        },
        None => n.e.el.name.clone(),
    }
}

/// The ability field of a granting rule: a `grant_spell`'s `ability`.
fn grant_ability_field(graph: &Graph<'_>, spell: &Node<'_>) -> Option<Value> {
    let Origin::Granted(ri) = spell.origin else {
        return None;
    };
    let parent = &graph.nodes[spell.parent?];
    let Some(Rule::Grant(g)) = parent.e.el.rules.get(ri) else {
        return None;
    };
    g.extra
        .attributes
        .get(X_SPELL_ABILITY)
        .map(|t| Value::String(t.clone()))
}

/// The ability of a feat-style element instance: an `ability` field of one of its custom rules
/// (`spell_free_cast`), else a parameter whose name ends in "spellcasting ability".
fn element_ability(graph: &Graph<'_>, owner: NodeId) -> Option<String> {
    let n = &graph.nodes[owner];
    let from_rules = n.e.el.rules.iter().find_map(|r| match r {
        Rule::Custom(c) => c
            .fields
            .get("ability")
            .and_then(|v| ability_of(graph, owner, v))
            .map(|(a, _)| a),
        _ => None,
    });
    from_rules.or_else(|| {
        n.slots.iter().find_map(|sl| {
            let Rule::Select(r) = &n.e.el.rules[sl.rule] else {
                return None;
            };
            let key = r.extra.attributes.get(crate::custom_rules::X_KEY)?;
            if crate::custom_rules::kind_of(r) != Some("param")
                || !key.ends_with("spellcasting_ability")
            {
                return None;
            }
            let v = graph.param_value(owner, key)?;
            crate::ids::ability_index(&v).map(|i| crate::ids::ABILITIES[i].to_string())
        })
    })
}

struct Pending {
    name: String,
    ability: Option<String>,
    spells: Vec<NodeId>,
}

fn add_to(pending: &mut Vec<Pending>, name: String, ability: Option<String>, spell: NodeId) {
    match pending.iter_mut().find(|p| p.name == name) {
        Some(p) => p.spells.push(spell),
        None => pending.push(Pending {
            name,
            ability,
            spells: vec![spell],
        }),
    }
}

/// Append the blocks of spells cast outside any class (see the module docs). Run after the
/// class and item blocks: a name already listed is never repeated.
pub fn add_granted_blocks(graph: &Graph<'_>, stats: &StatSet, out: &mut Vec<SpellcastingSnapshot>) {
    let mut pending: Vec<Pending> = Vec::new();
    for (id, n) in graph.nodes_of_type("Spell") {
        let Some(parent) = n.parent else { continue };
        if graph.nodes[parent].item_uid.is_some()
            || std::iter::successors(Some(parent), |p| graph.nodes[*p].parent)
                .any(|p| graph.nodes[p].item_uid.is_some())
        {
            continue;
        }
        match &n.spell_class {
            Some(class) if !out.iter().any(|b| &b.name == class) => {
                // without an ability the spells keep their place (no block with DC 0)
                let Some(ability) = element_ability(graph, parent) else {
                    continue;
                };
                let name = graph
                    .param_value(parent, &param_key("Spell List"))
                    .map_or_else(|| class.clone(), |l| format!("{class} ({l})"));
                add_to(&mut pending, name, Some(ability), id);
            }
            Some(_) => {}
            None => {
                let Some(field) = grant_ability_field(graph, n) else {
                    continue;
                };
                let Some((ability, Some(owner))) = ability_of(graph, parent, &field) else {
                    continue;
                };
                add_to(
                    &mut pending,
                    owner_block_name(graph, owner),
                    Some(ability),
                    id,
                );
            }
        }
    }
    for p in pending {
        if out.iter().any(|b| b.name == p.name) {
            continue;
        }
        let mut block = crate::item_spells::block(p.name, p.ability, stats);
        for id in p.spells {
            let n = &graph.nodes[id];
            if block.spells.iter().any(|s| s.id == n.e.el.id) {
                continue;
            }
            let mut entry = crate::item_spells::entry(graph, n);
            entry.always_prepared = n.always_prepared && entry.level > 0;
            block.spells.push(entry);
        }
        block
            .spells
            .sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.name.cmp(&b.name)));
        out.push(block);
    }
}
