//! Feats and prerequisites.

use super::mech::{feat_category_label, AbilitySource, Mech};
use super::optfeat::optional_feature_selects;
use super::prereq::apply_prerequisites;
use super::spell_blocks::{self, Carrier};
use super::spells_extra;
use super::{
    append_fluff, drain_renderer, new_element, note_unmapped, sheet_text, short_text, Conv, Ent,
    Shared, State,
};
use crate::ids::IdKind;
use crate::model::{self, Feat};
use crate::srd::SrdKind;
use crate::text::{RefKind, Renderer};
use borealis_content::{Rule, Setter};
use serde_json::Value;

/// Convert one feat.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let feat: Feat = match model::parse(&ent.raw) {
        Ok(f) => f,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "feat",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.named(IdKind::Feat, &ent.source, &feat.name);
    let mut el = new_element(
        &id,
        &feat.name,
        "Feat",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let mut partial: Vec<String> = Vec::new();

    if let Some(cat) = &feat.category {
        el.supports.push(feat_category_label(cat).to_string());
        el.data
            .insert("category".into(), Value::String(cat.clone()));
    }
    apply_prerequisites(sh, st, &feat.prerequisite, &mut el, &mut partial);

    let mut mech = Mech::new(sh, &feat.name);
    let source_display = sh.source_display(&ent.source);
    let linked = super::feat_saves::linked_choice(
        st,
        &feat,
        &super::feat_saves::FeatRef {
            id: &id,
            source: &ent.source,
            source_display: &source_display,
            ruleset: ent.ruleset,
        },
    );
    if linked.is_none() {
        mech.ability(&feat.ability, AbilitySource::Feat);
    }
    mech.skills(&feat.skill_proficiencies);
    mech.tools(&feat.tool_proficiencies);
    mech.languages(&feat.language_proficiencies);
    mech.weapons(&feat.weapon_proficiencies);
    mech.armor(&feat.armor_proficiencies);
    mech.skill_tool_language(&feat.skill_tool_language_proficiencies);
    mech.expertise(&feat.expertise);
    mech.resistances(
        &feat.resist,
        &feat.immune,
        &feat.vulnerable,
        &feat.condition_immune,
    );
    mech.senses(None, None, &feat.senses);
    for p in &feat.saving_throw_proficiencies {
        if let Some(map) = p.as_object() {
            for (k, v) in map {
                if v == &Value::Bool(true) {
                    mech.saving_throws(std::slice::from_ref(k));
                } else if k == "choose" && linked.is_none() {
                    partial.push("saving throw choice was not converted".into());
                }
            }
        }
    }
    let spells = spells_extra::convert_inheriting(
        sh,
        &feat.name,
        &feat.additional_spells,
        true,
        &spells_extra::inherited_abilities(&feat.ability),
    );
    let placed = spell_blocks::place(
        &Carrier {
            id: &id,
            name: &feat.name,
            source: &source_display,
            ruleset: ent.ruleset,
            free: true,
        },
        &spells.casting,
        &spells.direct,
        true,
    );
    for r in spells.direct.into_iter().chain(placed.rules) {
        mech.push(r);
    }
    el.spellcasting = placed.spellcasting;
    let mut elements_extra = placed.elements;
    if let Some(l) = linked {
        mech.push(l.rule);
        elements_extra.extend(l.options);
    }
    partial.extend(spells.partial);
    if !spells.options.is_empty() {
        for opt in spells.options {
            let oid = st.ids.id(
                IdKind::SpeciesOption,
                &ent.source,
                &[&feat.name, &opt.name],
                &format!("feat|{}|{}|{}", feat.name, ent.source, opt.name),
            );
            let mut oe = new_element(
                &oid,
                &opt.name,
                "Feat Feature",
                &source_display,
                ent.ruleset,
            );
            oe.supports = vec![id.clone()];
            oe.description = format!("<p>Option of {}.</p>", crate::text::tags::esc(&feat.name));
            oe.sheet = Some(borealis_content::Sheet {
                display: Some(false),
                ..borealis_content::Sheet::default()
            });
            let helpers = spell_blocks::fill(&mut oe, opt.rules, &opt.casting, true);
            elements_extra.push(oe);
            elements_extra.extend(helpers);
        }
        mech.push(Rule::Select(super::build::select(
            "Feat Feature",
            &format!("{} Option", feat.name),
            Some(&id),
            1,
        )));
    }
    for r in optional_feature_selects(&feat.optionalfeature_progression, &mut partial) {
        mech.push(r);
    }
    if feat.repeatable == Some(true) {
        el.setters.push(Setter::new("allow duplicate", "true"));
    }
    el.rules = std::mem::take(&mut mech.rules);
    for (k, vs) in &mech.unmapped {
        note_unmapped(&mut el, k, &vs.join("; "));
    }
    partial.extend(std::mem::take(&mut mech.partial));

    let mut description = renderer.entries(&feat.entries);
    append_fluff(sh, "feat", ent, &mut description, &mut renderer);
    el.description = description;
    el.sheet = Some(sheet_text(&short_text(
        &Renderer::plain(&feat.entries),
        300,
    )));
    el.maps_to = sh.maps_to(
        SrdKind::Feat,
        ent.ruleset,
        &feat.name,
        &feat.reprinted_as,
        &|uid| sh.reprint(RefKind::Feat, uid),
    );
    drain_renderer(st, &renderer, &ent.file, &feat.name);
    partial.sort();
    partial.dedup();
    let mut elements = vec![el];
    elements.extend(elements_extra);
    Conv { elements, partial }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Ruleset;
    use serde_json::json;

    fn ent(raw: Value) -> Ent {
        Ent {
            name: raw["name"].as_str().unwrap().to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: true,
            file: "feats.json".into(),
            raw,
        }
    }

    #[test]
    fn half_feat_with_ability_choice_and_repeatable() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(
            json!({"name": "Sturdy", "source": "ZZZ", "category": "G", "repeatable": true,
            "prerequisite": [{"level": 4}],
            "ability": [{"choose": {"from": ["str", "con"]}}],
            "skillProficiencies": [{"athletics": true}],
            "senses": [{"darkvision": 60}],
            "entries": ["You are sturdy."]}),
        );
        let conv = convert(&sh, &mut st, &e);
        let el = &conv.elements[0];
        assert!(conv.partial.is_empty(), "{:?}", conv.partial);
        assert_eq!(el.id, "ID_5ET_ZZZ_FEAT_STURDY");
        assert_eq!(el.supports, vec!["General Feat"]);
        assert_eq!(el.requirements, "[character:4]");
        assert_eq!(el.setter_value("allow duplicate"), Some("true"));
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Select(s) if s.ty == "Ability Score Improvement")));
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_PROFICIENCY_SKILL_ATHLETICS")));
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_VISION_DARKVISION")));
        assert_eq!(el.data["category"], "G");
        assert_eq!(el.ruleset, Some(Ruleset::Dnd2014));
    }

    #[test]
    fn magic_initiate_like_spell_options() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(
            json!({"name": "Dabbler", "source": "XPHB", "edition": "one", "category": "O",
            "additionalSpells": [
              {"name": "Cleric Spells", "ability": {"choose": ["int", "wis", "cha"]},
               "innate": {"_": {"daily": {"1": [{"choose": "level=1|class=Cleric"}]}}},
               "known": {"_": [{"choose": "level=0|class=Cleric", "count": 2}]}},
              {"name": "Wizard Spells", "known": {"_": [{"choose": "level=0|class=Wizard", "count": 2}]}}],
            "entries": ["Two cantrips."]}),
        );
        let conv = convert(&sh, &mut st, &e);
        let el = &conv.elements[0];
        // feat, option 1 + one spellcasting helper per ability, option 2 (block on itself)
        assert_eq!(conv.elements.len(), 6);
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Select(s) if s.ty == "Feat Feature")));
        let cleric = &conv.elements[1];
        assert_eq!(cleric.ty, "Feat Feature");
        assert_eq!(cleric.supports, vec![el.id.clone()]);
        assert!(cleric.spellcasting.is_none());
        assert!(cleric.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Grants" && s.name == "Spellcasting Ability (Dabbler: Cleric Spells)")));
        assert!(cleric
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Custom(c) if c.kind == "spell_free_cast")));
        let wis = &conv.elements[3];
        assert_eq!(wis.ty, "Grants");
        let block = wis.spellcasting.as_ref().unwrap();
        assert_eq!(
            (block.name.as_deref(), block.ability.as_deref()),
            (Some("Dabbler: Cleric Spells"), Some("Wisdom"))
        );
        let wizard = &conv.elements[5];
        assert_eq!(
            wizard.spellcasting.as_ref().and_then(|b| b.name.as_deref()),
            Some("Dabbler: Wizard Spells")
        );
        assert_eq!(el.ruleset, Some(Ruleset::Dnd2024));
        assert_eq!(el.supports, vec!["Origin Feat"]);
    }

    #[test]
    fn unmapped_prerequisites_are_reported() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(
            json!({"name": "Elfy", "source": "ZZZ", "prerequisite": [{"race": [{"name": "elf"}]}], "entries": ["x"]}),
        );
        let conv = convert(&sh, &mut st, &e);
        assert_eq!(conv.partial.len(), 1);
        assert!(conv.partial[0].contains("Race: Elf"));
        assert!(conv.elements[0].prerequisite.ends_with("(not checked)"));
        let bad = ent(json!({"name": "Bad", "source": "ZZZ", "entries": "text"}));
        assert!(convert(&sh, &mut st, &bad).elements.is_empty());
    }
}
