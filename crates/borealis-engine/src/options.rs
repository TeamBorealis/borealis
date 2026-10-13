//! Option lists of select slots (`engine.md` 6.2).

use crate::graph::{Graph, NodeId, SpellDynamics};
use crate::universe::UElem;
use borealis_content::Rule;
use std::cmp::Ordering;

impl<'u> Graph<'u> {
    /// Candidates a slot currently offers, in display order.
    ///
    /// Filters in order: type, ruleset compatibility, supports (with dynamic spell tokens), source
    /// restriction, duplicate removal (the slot's own choice stays), element requirements
    /// (`[level:n]` reads the owning class level). Sorting: spells by (level, name),
    /// alignments and ability score selects in file order, everything else by name.
    #[must_use]
    pub fn slot_options(&self, owner: NodeId, slot_index: usize) -> Vec<&'u UElem> {
        let n = &self.nodes[owner];
        let slot = &n.slots[slot_index];
        let Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
            return Vec::new();
        };
        let current = slot.chosen.map(|c| self.nodes[c].e.el.id.as_str());
        let uni = self.uni;
        let mut out: Vec<&'u UElem> = uni
            .candidates_for(&rule.ty)
            .filter(|c| {
                if current == Some(c.el.id.as_str()) {
                    return true;
                }
                self.choice_allowed(owner, slot.rule, c, false)
            })
            .collect();
        for alt in crate::custom_rules::alternatives_of(rule) {
            if let Some(c) = uni.get(alt) {
                if !out.iter().any(|o| o.el.id == c.el.id)
                    && (current == Some(alt) || self.choice_allowed(owner, slot.rule, c, false))
                {
                    out.push(c);
                }
            }
        }
        if rule.ty == "Companion" {
            out = prefer_companions(out, current);
        }
        let keep_order = rule.ty == "Alignment"
            || (rule.name.contains("Ability Score")
                && matches!(
                    rule.ty.as_str(),
                    "Racial Trait" | "Class Feature" | "Ability Score Improvement"
                ));
        if rule.ty == "Spell" {
            out.sort_by(|a, b| {
                a.spell_level()
                    .cmp(&b.spell_level())
                    .then_with(|| a.el.name.to_lowercase().cmp(&b.el.name.to_lowercase()))
            });
        } else if !keep_order {
            out.sort_by(|a, b| cmp_names(&a.el.name, &b.el.name));
        }
        out
    }

    /// Dynamic spell token context for a node's rule (exposed for the snapshot builder).
    #[must_use]
    pub fn spell_dynamics(&self, owner: NodeId, rule_index: usize) -> SpellDynamics<'_, 'u> {
        SpellDynamics::new(self, owner, rule_index)
    }
}

/// A `Companion` select offers content companions first: a typed `Monster` stat block is
/// dropped when a `Companion` (or an earlier monster) of the same name is offered, unless it is
/// the slot's current choice.
fn prefer_companions<'u>(out: Vec<&'u UElem>, current: Option<&str>) -> Vec<&'u UElem> {
    let mut seen: std::collections::HashSet<String> = out
        .iter()
        .filter(|c| c.el.ty != "Monster")
        .map(|c| c.el.name.to_lowercase())
        .collect();
    out.into_iter()
        .filter(|c| {
            c.el.ty != "Monster"
                || current == Some(c.el.id.as_str())
                || seen.insert(c.el.name.to_lowercase())
        })
        .collect()
}

fn cmp_names(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}
