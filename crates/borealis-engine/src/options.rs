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
        let dyns = SpellDynamics::new(self, owner, slot.rule);
        let allowed = |c: &UElem| self.choice_allowed_with(owner, slot.rule, c, false, &dyns);
        let mut out: Vec<&'u UElem> = uni
            .candidates_for(&rule.ty)
            .filter(|c| current == Some(c.el.id.as_str()) || allowed(c))
            .collect();
        for alt in crate::custom_rules::alternatives_of(rule) {
            if let Some(c) = uni.get(alt) {
                if !out.iter().any(|o| o.el.id == c.el.id) && (current == Some(alt) || allowed(c)) {
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

#[cfg(test)]
mod tests {
    use crate::commands::{self, slot_key};
    use crate::derive::Evaluation;
    use crate::state::{CharacterState, Choice};
    use crate::Engine;
    use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
    use serde_json::json;
    use std::path::Path;

    const RACE: &str = "ID_T_RACE_EXTENDS_WIZARD";

    fn engine() -> Engine {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        // a species that extends the Wizard list by two spell ids (one above the slot level)
        // and by an expression
        let race = json!({"id": RACE, "name": RACE, "type": "Race", "source": "Test",
            "spellcasting": {"name": "Wizard", "extend": true, "extends": [
                {"supports": "ID_PHB_SPELL_BLESS"}, {"supports": "ID_PHB_SPELL_HEAL"},
                {"supports": "Druid"}]}});
        let file: ContentFile = serde_json::from_value(
            json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": [race]}),
        )
        .expect("synthetic content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .expect("bundled packs")
            .add_file("synthetic.json", PackOrigin::Core, file)
            .build();
        Engine::new(&store)
    }

    fn wizard(e: &Engine, level: u32) -> CharacterState {
        let s = commands::new_character(e, Ruleset::Dnd2014);
        let pick = |s: &CharacterState, name: &str, id: &str| {
            let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).expect("slot");
            commands::select(e, s, &key, Choice::Element(id.into()))
                .expect("select")
                .state
        };
        let mut s = pick(&pick(&s, "Race", RACE), "Class", "ID_WOTC_PHB_CLASS_WIZARD");
        while s.level() < level {
            s = commands::level_up_main(e, &s, None)
                .expect("level up")
                .state;
        }
        s
    }

    /// The option list prepares the spell token context once per slot; it must offer exactly
    /// what the per-candidate check (`choice_allowed`, a fresh context each time) allows.
    #[test]
    fn option_lists_match_the_per_candidate_check() {
        let e = engine();
        let state = wizard(&e, 5);
        let uni = e.universe(Ruleset::Dnd2014);
        let ev = Evaluation::run(uni, &state, e.settings(), false);
        let g = &ev.graph;
        let mut spell_slots = 0;
        for id in g.preorder() {
            let n = &g.nodes[id];
            for (si, slot) in n.slots.iter().enumerate() {
                let borealis_content::Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
                    continue;
                };
                let fast: Vec<&str> = g
                    .slot_options(id, si)
                    .iter()
                    .map(|c| c.el.id.as_str())
                    .collect();
                let current = slot.chosen.map(|c| g.nodes[c].e.el.id.as_str());
                let mut slow: Vec<&str> = uni
                    .candidates_for(&rule.ty)
                    .filter(|c| {
                        current == Some(c.el.id.as_str())
                            || g.choice_allowed(id, slot.rule, c, false)
                    })
                    .map(|c| c.el.id.as_str())
                    .collect();
                let mut sorted = fast.clone();
                sorted.sort_unstable();
                slow.sort_unstable();
                assert_eq!(sorted, slow, "slot {} of {}", rule.name, n.e.el.id);
                if rule.ty == "Spell"
                    && rule.level.unwrap_or(1) == 5
                    && !rule.name.contains("Cantrip")
                {
                    spell_slots += 1;
                    assert!(fast.contains(&"ID_PHB_SPELL_BLESS"), "extension by id");
                    assert!(
                        !fast.contains(&"ID_PHB_SPELL_HEAL"),
                        "id above the slot level"
                    );
                    assert!(
                        fast.contains(&"ID_PHB_SPELL_CURE_WOUNDS"),
                        "extension by expression"
                    );
                }
            }
        }
        assert!(
            spell_slots > 0,
            "the level-5 wizard spell selects were checked"
        );
    }
}
