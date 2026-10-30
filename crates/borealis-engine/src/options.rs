//! Option lists of select slots (`engine.md` 6.2).

use crate::graph::{Graph, NodeId, Relax, SpellDynamics};
use crate::snapshot::{EmptyReason, OtherEditionOptions, SlotProblem};
use crate::universe::EditionStatus;
use crate::universe::UElem;
use crate::universe::{Cond, URule};
use borealis_content::Rule;

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
            .select_candidates(&rule.ty, self.allow_cross_edition)
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
        // stable sorts on keys computed once per candidate (a comparator that lowercases both
        // names allocated twice per comparison: thousands of spells made that the hot path)
        if rule.ty == "Spell" {
            out.sort_by_cached_key(|c| (c.spell_level(), c.el.name.to_lowercase()));
        } else if !keep_order {
            out.sort_by_cached_key(|c| c.el.name.to_lowercase());
        }
        out
    }

    /// Does the slot offer anything at all? The same filters as [`Self::slot_options`]
    /// (`slot_options(..).is_empty()` is `!slot_has_options(..)`), but it stops at the first
    /// candidate and sorts nothing: a derive without option lists asks this for every open slot.
    /// The list's later steps never empty it (sorting, and the companion preference, which keeps
    /// every non-monster and the first monster of each name).
    #[must_use]
    pub fn slot_has_options(&self, owner: NodeId, slot_index: usize) -> bool {
        let n = &self.nodes[owner];
        let slot = &n.slots[slot_index];
        let Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
            return false;
        };
        let current = slot.chosen.map(|c| self.nodes[c].e.el.id.as_str());
        let uni = self.uni;
        let dyns = SpellDynamics::new(self, owner, slot.rule);
        let allowed = |c: &UElem| self.choice_allowed_with(owner, slot.rule, c, false, &dyns);
        // an alternative is the current choice by the id the rule names, as in the option list
        // (a proxy id resolves to an element that may carry another spelling of it)
        uni.select_candidates(&rule.ty, self.allow_cross_edition)
            .any(|c| current == Some(c.el.id.as_str()) || allowed(c))
            || crate::custom_rules::alternatives_of(rule)
                .into_iter()
                .filter_map(|a| uni.get(a).map(|c| (a, c)))
                .any(|(a, c)| current == Some(a) || allowed(c))
    }

    /// The slot offers nothing only because the character already has every element it would
    /// offer (Hobgoblin's two martial weapons, Weapon Master on a fighter): the choice is done.
    /// Call it for a slot whose option list is empty; a slot that matches nothing at all (a
    /// content error) is not "all known".
    #[must_use]
    pub fn slot_all_known(&self, owner: NodeId, slot_index: usize) -> bool {
        let n = &self.nodes[owner];
        let slot = &n.slots[slot_index];
        let Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
            return false;
        };
        if rule.is_list() {
            return false;
        }
        let uni = self.uni;
        let dyns = SpellDynamics::new(self, owner, slot.rule);
        let held = |c: &UElem| {
            self.ids.count(&c.el.id) > 0
                && self.choice_allowed_inner(owner, slot.rule, c, false, &dyns, true)
        };
        uni.select_candidates(&rule.ty, self.allow_cross_edition)
            .any(held)
            || crate::custom_rules::alternatives_of(rule)
                .into_iter()
                .filter_map(|a| uni.get(a))
                .any(held)
    }

    /// Why a slot that offers nothing is empty (U14): the candidates that pass every filter but
    /// the edition, and those that pass every filter but the source restriction. `None` when
    /// neither hides anything (nothing matches at all). Call it for an empty, open slot.
    #[must_use]
    pub fn slot_empty_reason(&self, owner: NodeId, slot_index: usize) -> Option<EmptyReason> {
        let n = &self.nodes[owner];
        let slot = &n.slots[slot_index];
        let Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
            return None;
        };
        if rule.is_list() {
            return None;
        }
        let uni = self.uni;
        let dyns = SpellDynamics::new(self, owner, slot.rule);
        let check = |c: &UElem, relax: Relax| {
            self.choice_allowed_relaxed(owner, slot.rule, c, false, &dyns, false, relax)
        };
        let alternatives = crate::custom_rules::alternatives_of(rule)
            .into_iter()
            .filter_map(|a| uni.get(a));
        let mut seen = std::collections::HashSet::new();
        let mut reason = EmptyReason::default();
        let mut other: Option<OtherEditionOptions> = None;
        for c in uni.select_candidates(&rule.ty, true).chain(alternatives) {
            if !seen.insert(c.el.id.as_str()) || check(c, Relax::NONE) {
                continue;
            }
            let foreign = match &c.edition {
                EditionStatus::Foreign { from, .. } => Some(*from),
                EditionStatus::Native => None,
            };
            let by_edition = Relax {
                edition: true,
                ..Relax::NONE
            };
            if let Some(from) = foreign.filter(|_| check(c, by_edition)) {
                let o = other.get_or_insert(OtherEditionOptions {
                    edition: from,
                    rejected: 0,
                    not_allowed: 0,
                });
                if c.edition.is_accepted() {
                    o.not_allowed += 1;
                } else {
                    o.rejected += 1;
                }
            } else if check(
                c,
                Relax {
                    restriction: true,
                    ..Relax::NONE
                },
            ) {
                reason.restricted += 1;
            }
        }
        reason.other_edition = other;
        (reason != EmptyReason::default()).then_some(reason)
    }

    /// Content behind a choice that can't be read (GitHub #11, C1): the select's own `supports`,
    /// or candidates that pass every filter but their own unreadable requirements (never
    /// offered). `None` when there is nothing to say.
    #[must_use]
    pub fn slot_problem(&self, owner: NodeId, slot_index: usize) -> Option<SlotProblem> {
        let n = &self.nodes[owner];
        let slot = &n.slots[slot_index];
        let (Rule::Select(rule), URule::Select { supports, .. }) =
            (&n.e.el.rules[slot.rule], &n.e.rules[slot.rule])
        else {
            return None;
        };
        if matches!(supports, Cond::Invalid) {
            return Some(SlotProblem {
                message: format!(
                    "This choice's filter, \"{}\", can't be read, so it offers nothing.",
                    rule.supports.as_deref().unwrap_or_default().trim()
                ),
                elements: vec![n.e.el.id.clone()],
            });
        }
        if rule.is_list() {
            return None;
        }
        let dyns = SpellDynamics::new(self, owner, slot.rule);
        let relax = Relax {
            requirements: true,
            ..Relax::NONE
        };
        let mut seen = std::collections::HashSet::new();
        let hidden: Vec<&UElem> = self
            .uni
            .select_candidates(&rule.ty, self.allow_cross_edition)
            .filter(|c| matches!(c.req, Cond::Invalid) && seen.insert(c.el.id.as_str()))
            .filter(|c| {
                self.choice_allowed_relaxed(owner, slot.rule, c, false, &dyns, false, relax)
            })
            .collect();
        if hidden.is_empty() {
            return None;
        }
        let mut names: Vec<&str> = hidden
            .iter()
            .map(|c| c.el.name.as_str())
            .take(MAX_PROBLEM_NAMES)
            .collect();
        if hidden.len() > MAX_PROBLEM_NAMES {
            names.push("…");
        }
        let (count, its) = if hidden.len() == 1 {
            ("1 option".to_string(), "its")
        } else {
            (format!("{} options", hidden.len()), "their")
        };
        Some(SlotProblem {
            message: format!(
                "{count} can't be offered: {its} requirements can't be read ({}).",
                names.join(", ")
            ),
            elements: hidden.iter().map(|c| c.el.id.clone()).collect(),
        })
    }

    /// Dynamic spell token context for a node's rule (exposed for the snapshot builder).
    #[must_use]
    pub fn spell_dynamics(&self, owner: NodeId, rule_index: usize) -> SpellDynamics<'_, 'u> {
        SpellDynamics::new(self, owner, rule_index)
    }
}

/// Option names a slot problem lists before "…".
const MAX_PROBLEM_NAMES: usize = 5;

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
                    .select_candidates(&rule.ty, true)
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

    const PICKY: &str = "ID_T_RACE_PICKY";

    /// A species with a select no element satisfies and a feat select over feats whose names
    /// differ in case (the bundled packs plus this synthetic content).
    fn picky_engine() -> Engine {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let feat = |id: &str, name: &str| json!({"id": id, "name": name, "type": "Feat", "source": "Test"});
        let race = json!({"id": PICKY, "name": "Picky", "type": "Race", "source": "Test", "rules": [
            {"kind": "select", "type": "Spell", "name": "Nothing", "supports": "ID_T_NO_SUCH_SPELL"},
            {"kind": "select", "type": "Feat", "name": "Picky Feat", "level": 1}]});
        let elements = vec![
            race,
            feat("ID_T_FEAT_B", "beta test"),
            feat("ID_T_FEAT_A", "Alpha test"),
            feat("ID_T_FEAT_G", "gamma test"),
            feat("ID_T_FEAT_D", "Delta test"),
        ];
        let file: ContentFile = serde_json::from_value(
            json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements}),
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

    fn picky_wizard(e: &Engine, level: u32) -> CharacterState {
        let s = commands::new_character(e, Ruleset::Dnd2014);
        let pick = |s: &CharacterState, name: &str, id: &str| {
            let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).expect("slot");
            commands::select(e, s, &key, Choice::Element(id.into()))
                .expect("select")
                .state
        };
        let mut s = pick(
            &pick(&s, "Race", PICKY),
            "Class",
            "ID_WOTC_PHB_CLASS_WIZARD",
        );
        while s.level() < level {
            s = commands::level_up_main(e, &s, None)
                .expect("level up")
                .state;
        }
        s
    }

    /// A derive without option lists asks every open slot whether it offers anything
    /// (`slot_has_options`, no list built or sorted): the answer must be the option list's.
    #[test]
    fn slot_has_options_agrees_with_the_option_list() {
        let e = picky_engine();
        let uni = e.universe(Ruleset::Dnd2014);
        let (mut empty, mut offered) = (0, 0);
        for level in [1, 3, 5] {
            let state = picky_wizard(&e, level);
            let ev = Evaluation::run(uni, &state, e.settings(), false);
            let g = &ev.graph;
            for id in g.preorder() {
                let n = &g.nodes[id];
                for (si, slot) in n.slots.iter().enumerate() {
                    let borealis_content::Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
                        continue;
                    };
                    let listed = !g.slot_options(id, si).is_empty();
                    assert_eq!(
                        g.slot_has_options(id, si),
                        listed,
                        "slot {} of {} at level {level}",
                        rule.name,
                        n.e.el.id
                    );
                    if listed {
                        offered += 1;
                    } else {
                        empty += 1;
                    }
                }
            }
        }
        assert!(empty > 0 && offered > 0, "both answers were checked");
        // the empty select is reported as such by a derive without option lists, too
        let snap = e.derive_with(&picky_wizard(&e, 1), false);
        let nothing = snap
            .slots
            .iter()
            .find(|s| s.name == "Nothing")
            .expect("slot");
        assert!(!nothing.all_known && nothing.requires_selection);
    }

    /// Option lists sort by name ignoring case (spells by level, then name), as before the sort
    /// keys were cached.
    #[test]
    fn option_lists_sort_by_name_ignoring_case_and_spells_by_level() {
        let e = picky_engine();
        let snap = e.derive(&picky_wizard(&e, 5));
        let feats: Vec<&str> = snap
            .slots
            .iter()
            .find(|s| s.name == "Picky Feat")
            .expect("feat slot")
            .options
            .iter()
            .map(|o| o.name.as_str())
            .filter(|n| n.ends_with(" test"))
            .collect();
        assert_eq!(
            feats,
            ["Alpha test", "beta test", "Delta test", "gamma test"]
        );
        let mut spell_lists = 0;
        for slot in snap
            .slots
            .iter()
            .filter(|s| s.ty == "Spell" && s.options.len() > 1)
        {
            spell_lists += 1;
            let keys: Vec<(u32, String)> = slot
                .options
                .iter()
                .map(|o| (o.level.unwrap_or(0), o.name.to_lowercase()))
                .collect();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(keys, sorted, "{}", slot.name);
        }
        assert!(spell_lists > 0, "a spell list was checked");
    }
}
