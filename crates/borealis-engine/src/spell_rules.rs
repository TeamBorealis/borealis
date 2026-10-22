//! 2024 spellcasting rule kinds that are read from live elements: `spellbook`, `mystic_arcanum`,
//! `pact_magic`, `prepared_spell_change`, `ritual_casting`, `spell_list_extend`, the free casts of
//! `grant_spell` and the named `save_dc` rules.

use crate::custom_rules::{X_ALSO_WITH_SLOTS, X_CAST_LEVEL, X_FREE_CAST};
use crate::graph::{Graph, Origin};
use crate::snapshot::{FreeCastSnapshot, NamedSaveDc, ResourceSnapshot, SpellcastingSnapshot};
use crate::universe::Universe;
use borealis_content::Rule;
use serde_json::Value;

/// Suffix of the state key under which a class keeps its spellbook (`Wizard#spellbook`).
pub const SPELLBOOK_SUFFIX: &str = "#spellbook";

/// State key of a class spellbook.
#[must_use]
pub fn spellbook_key(class: &str) -> String {
    format!("{class}{SPELLBOOK_SUFFIX}")
}

/// The `spellbook` rule.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpellbookRule {
    pub initial: u32,
    pub per_level: u32,
    pub cost_gp: u32,
    pub hours: u32,
}

/// Spellcasting rules attached to one class.
#[derive(Debug, Clone, Default)]
pub struct ClassSpellRules {
    pub spellbook: Option<SpellbookRule>,
    /// `(spell level, class level)` pairs.
    pub arcanum: Vec<(u32, u32)>,
    pub arcanum_recharge: String,
    pub pact: bool,
    pub prepared_change: Option<(String, String)>,
    pub ritual_source: Option<String>,
    pub extend_lists: Vec<String>,
}

fn num(c: &Value, key: &str) -> u32 {
    c.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

fn text(c: &Value, key: &str) -> String {
    c.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Collect the rules of the live elements that belong to `class` (the Spellcasting feature or any
/// class feature whose `data.class` names it).
#[must_use]
pub fn class_rules(graph: &Graph<'_>, class: &str) -> ClassSpellRules {
    let mut out = ClassSpellRules::default();
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        let owns = el.data.get("class").and_then(Value::as_str) == Some(class)
            || el.spellcasting.as_ref().and_then(|s| s.name.as_deref()) == Some(class);
        for rule in &el.rules {
            let Rule::Custom(c) = rule else { continue };
            let v = Value::Object(c.fields.clone());
            match c.kind.as_str() {
                "spellbook" if owns => {
                    out.spellbook = Some(SpellbookRule {
                        initial: num(&v, "initial_spells"),
                        per_level: num(&v, "per_level"),
                        cost_gp: num(&v, "copy_cost_gp_per_level"),
                        hours: num(&v, "copy_hours_per_level"),
                    });
                }
                "mystic_arcanum" if owns => {
                    out.arcanum_recharge = v
                        .pointer("/free_cast/recharge")
                        .and_then(Value::as_str)
                        .unwrap_or("long_rest")
                        .to_string();
                    for s in v
                        .get("slots")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        out.arcanum.push((num(s, "spell_level"), num(s, "level")));
                    }
                }
                "pact_magic" if owns => out.pact = true,
                "prepared_spell_change" if owns => {
                    out.prepared_change = Some((text(&v, "when"), text(&v, "count")));
                }
                "ritual_casting" if owns => out.ritual_source = Some(text(&v, "source")),
                "spell_list_extend" if text(&v, "spell_class") == class => {
                    for l in v
                        .get("lists")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(l) = l.as_str() {
                            out.extend_lists.push(l.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// A value of the `Class.data.progression` table at a class level (1-based).
#[must_use]
pub fn progression(uni: &Universe, class: &str, column: &str, class_level: u32) -> Option<i32> {
    let idx = usize::try_from(class_level.checked_sub(1)?).ok()?;
    let class_el = uni.class_named(class)?;
    class_el
        .el
        .data
        .get("progression")?
        .get(column)?
        .as_array()?
        .get(idx)?
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
}

/// Uses of a `free_cast` object: the number (when the engine resolves it) and the authored rule.
fn free_cast_uses(v: &Value, prof: i32, mods: &[i32; 6]) -> (Option<i32>, String) {
    match v.get("uses") {
        Some(Value::Number(k)) => (k.as_i64().and_then(|k| i32::try_from(k).ok()), k_text(k)),
        Some(Value::String(s)) if s == "proficiency" => (Some(prof), s.clone()),
        // an ability name: uses equal to its modifier, at least one
        Some(Value::String(s)) if ABILITIES.contains(&s.as_str()) => {
            let i = ABILITIES.iter().position(|a| a == s).unwrap_or(0);
            (Some(mods[i].max(1)), s.clone())
        }
        Some(t) => (
            None,
            format!(
                "table:{}",
                t.get("key").and_then(Value::as_str).unwrap_or("?")
            ),
        ),
        None => (None, String::new()),
    }
}

/// Uses read from a column of the owning class's progression table (`{"type": "table", "key":
/// "favored_enemy"}`: SRD 5.2.1 Ranger Favored Enemy, 2 casts at Ranger 1 to 4 at Ranger 17) at
/// that class's level.
fn table_uses(graph: &Graph<'_>, granter: usize, v: &Value) -> Option<i32> {
    let uses = v.get("uses")?;
    if uses.get("type").and_then(Value::as_str) != Some("table") {
        return None;
    }
    let key = uses.get("key").and_then(Value::as_str)?;
    let class = std::iter::successors(Some(granter), |id| graph.nodes[*id].parent)
        .map(|id| &graph.nodes[id].e.el)
        .find(|el| matches!(el.ty.as_str(), "Class" | "Multiclass"))
        .map(|el| el.name.clone())?;
    let level = graph.class_levels.get(&class.to_lowercase()).copied()?;
    progression(graph.uni, &class, key, u32::try_from(level).ok()?)
}

/// The `spell_free_cast` rule of `parent` bound to the Spell select that filled slot `si`.
fn select_free_cast(graph: &Graph<'_>, parent: usize, si: usize) -> Option<Value> {
    let p = &graph.nodes[parent];
    let ri = usize::try_from(p.slots.get(si)?.key.rule).ok()?;
    let Some(Rule::Select(sel)) = p.e.el.rules.get(ri) else {
        return None;
    };
    p.e.el.rules.iter().find_map(|r| match r {
        Rule::Custom(c)
            if c.kind == "spell_free_cast"
                && c.fields.get("select").and_then(Value::as_str) == Some(sel.name.as_str()) =>
        {
            Some(Value::Object(c.fields.clone()))
        }
        _ => None,
    })
}

/// Limited free casts granted to the character: `grant_spell.free_cast` on a grant, or a
/// `spell_free_cast` rule bound to the Spell select the spell was picked in.
#[must_use]
pub fn free_casts(graph: &Graph<'_>, prof: i32, mods: &[i32; 6]) -> Vec<FreeCastSnapshot> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        let n = &graph.nodes[id];
        let Some(parent) = n.parent else { continue };
        let (v, also_with_slots, cast_level) = match n.origin {
            Origin::Granted(ri) => {
                let Some(Rule::Grant(g)) = graph.nodes[parent].e.el.rules.get(ri) else {
                    continue;
                };
                let Some(raw) = g.extra.attributes.get(X_FREE_CAST) else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<Value>(raw) else {
                    continue;
                };
                let also = g
                    .extra
                    .attributes
                    .get(X_ALSO_WITH_SLOTS)
                    .is_some_and(|s| s == "true");
                let cast_level = g
                    .extra
                    .attributes
                    .get(X_CAST_LEVEL)
                    .and_then(|s| s.parse::<u32>().ok());
                (v, also, cast_level)
            }
            Origin::Selected(si) if n.e.el.ty == "Spell" => {
                let Some(rule) = select_free_cast(graph, parent, si) else {
                    continue;
                };
                let also = rule.get("also_with_slots") == Some(&Value::Bool(true));
                (rule, also, None)
            }
            _ => continue,
        };
        let (uses, uses_rule) = match free_cast_uses(&v, prof, mods) {
            (None, rule) => (table_uses(graph, parent, &v), rule),
            known => known,
        };
        out.push(FreeCastSnapshot {
            spell: n.e.el.id.clone(),
            name: n.e.el.name.clone(),
            uses,
            uses_rule,
            recharge: text(&v, "recharge"),
            also_with_slots,
            cast_level,
            source: graph.nodes[parent].e.el.id.clone(),
        });
    }
    out
}

/// Id of the use pool of one free cast (`free_cast:<granting element>:<spell>`).
#[must_use]
pub fn free_cast_pool_id(f: &FreeCastSnapshot) -> String {
    format!("free_cast:{}:{}", f.source, f.spell)
}

/// The free casts as resource pools, so Play spends them and the rests restore them like any
/// other limited-use resource. A cast without a known positive count (a table the engine does
/// not resolve) gets no pool; a `none` recharge never comes back; the rest refill on a Long Rest
/// and, for `short_rest`, on a Short Rest too.
#[must_use]
pub fn free_cast_resources(graph: &Graph<'_>, casts: &[FreeCastSnapshot]) -> Vec<ResourceSnapshot> {
    let mut out: Vec<ResourceSnapshot> = Vec::new();
    for f in casts {
        let Some(max) = f.uses.filter(|n| *n > 0) else {
            continue;
        };
        let id = free_cast_pool_id(f);
        if out.iter().any(|r| r.id == id) {
            continue;
        }
        let used = i32::try_from(graph.runtime.used_of(&id)).unwrap_or(i32::MAX);
        let source = graph
            .nodes
            .iter()
            .find(|n| n.e.el.id == f.source)
            .map_or_else(|| f.source.clone(), |n| n.e.el.name.clone());
        out.push(ResourceSnapshot {
            id,
            name: format!("{} (free cast)", f.name),
            max,
            current: (max - used).max(0),
            source,
            recharge: if f.recharge.is_empty() {
                Vec::new()
            } else {
                vec![f.recharge.clone()]
            },
            short_rest_regain: (f.recharge == "short_rest").then_some(max),
            long_rest_regain_all: f.recharge != "none",
            other_regain: Vec::new(),
            rolled: None,
        });
    }
    out
}

fn k_text(k: &serde_json::Number) -> String {
    k.to_string()
}

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// Evaluate `8+strength+proficiency` / `spell save DC (Cleric)` style formulas. An ability term
/// may also be written `<ability>:modifier`, the form resource formulas use.
fn eval_formula(
    formula: &str,
    mods: &[i32; 6],
    prof: i32,
    casters: &[SpellcastingSnapshot],
) -> Option<i32> {
    let f = formula.trim().to_lowercase();
    if let Some(rest) = f.strip_prefix("spell save dc (") {
        let name = rest.trim_end_matches(')');
        return casters
            .iter()
            .find(|c| c.name.to_lowercase() == name)
            .map(|c| c.dc);
    }
    let mut total = 0;
    for term in f.split('+') {
        let t = term.trim();
        total += if let Ok(n) = t.parse::<i32>() {
            n
        } else if t == "proficiency" || t == "proficiency bonus" {
            prof
        } else {
            let ability = t.strip_suffix(":modifier").map_or(t, str::trim);
            mods[ABILITIES.iter().position(|a| *a == ability)?]
        };
    }
    Some(total)
}

/// Named save DCs of the live elements.
#[must_use]
pub fn save_dcs(
    graph: &Graph<'_>,
    mods: &[i32; 6],
    prof: i32,
    casters: &[SpellcastingSnapshot],
) -> Vec<NamedSaveDc> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        let el = &graph.nodes[id].e.el;
        for rule in &el.rules {
            let Rule::Custom(c) = rule else { continue };
            if c.kind != "save_dc" {
                continue;
            }
            let v = Value::Object(c.fields.clone());
            let formula = text(&v, "formula");
            out.push(NamedSaveDc {
                owner: el.id.clone(),
                id: text(&v, "id"),
                value: eval_formula(&formula, mods, prof, casters),
                formula,
            });
        }
    }
    out
}

#[cfg(test)]
mod save_dc_tests {
    use super::eval_formula;

    #[test]
    fn save_dc_terms_read_abilities_in_both_forms() {
        let mods = [0, 1, 3, 0, 2, 0];
        assert_eq!(
            eval_formula("8 + proficiency + constitution", &mods, 2, &[]),
            Some(13)
        );
        assert_eq!(
            eval_formula("8 + proficiency + constitution:modifier", &mods, 2, &[]),
            Some(13)
        );
        assert_eq!(
            eval_formula("8+proficiency bonus+wisdom", &mods, 3, &[]),
            Some(13)
        );
        assert_eq!(eval_formula("8 + con", &mods, 2, &[]), None);
    }
}
