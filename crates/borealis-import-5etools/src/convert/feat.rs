//! Feats and prerequisites.

use super::mech::{feat_category_label, AbilitySource, Mech};
use super::optfeat::optional_feature_selects;
use super::spell_blocks::{self, Carrier};
use super::spells_extra;
use super::{
    append_fluff, drain_renderer, new_element, note_unmapped, sheet_text, short_text, Conv, Ent,
    Shared, State,
};
use crate::core_ids;
use crate::ids::IdKind;
use crate::model::{self, Feat};
use crate::srd::SrdKind;
use crate::text::{RefKind, Renderer};
use borealis_content::{Rule, Setter};
use serde_json::Value;

/// A converted prerequisite.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Prereq {
    /// Display text.
    pub text: String,
    /// Requirements expression (empty when nothing can be enforced).
    pub requirements: String,
    /// Prerequisites that are shown but not enforced.
    pub unmapped: Vec<String>,
}

fn level_alt(v: &Value) -> Option<(String, String)> {
    match v {
        Value::Number(n) => {
            let n = n.as_u64()?;
            Some((format!("[character:{n}]"), format!("Level {n}+")))
        }
        Value::Object(_) => {
            let n = v.get("level").and_then(Value::as_u64)?;
            match v
                .get("class")
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
            {
                Some(class) => Some((
                    format!("[{}:{n}]", class.to_lowercase()),
                    format!("{class} level {n}+"),
                )),
                None => Some((format!("[character:{n}]"), format!("Level {n}+"))),
            }
        }
        _ => None,
    }
}

fn describe(key: &str, value: &Value) -> String {
    let flat = match value {
        Value::Bool(true) => String::new(),
        Value::String(s) => crate::text::strip_tags(s),
        Value::Array(a) => a
            .iter()
            .map(|x| match x {
                Value::String(s) => crate::text::strip_tags(s),
                Value::Object(o) => o
                    .get("displayEntry")
                    .or_else(|| o.get("name"))
                    .or_else(|| o.get("entry"))
                    .and_then(Value::as_str)
                    .map_or_else(|| x.to_string(), crate::text::strip_tags),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    };
    let label = match key {
        "spellcasting" | "spellcasting2020" | "spellcastingFeature" | "spellcastingPrepared" => {
            "Spellcasting".to_string()
        }
        "other" | "otherSummary" => return flat,
        k => crate::util::title_case(k),
    };
    if flat.is_empty() {
        label
    } else {
        format!("{label}: {flat}")
    }
}

/// Convert a `prerequisite` array (alternatives, each an AND of keys).
#[must_use]
pub fn prerequisites(items: &[Value]) -> Prereq {
    let mut texts = Vec::new();
    let mut exprs: Vec<Option<String>> = Vec::new();
    let mut unmapped = Vec::new();
    for item in items {
        let Some(map) = item.as_object() else {
            continue;
        };
        let mut parts: Vec<String> = Vec::new();
        let mut alt_text: Vec<String> = Vec::new();
        for (k, v) in map {
            match k.as_str() {
                "level" => {
                    if let Some((expr, text)) = level_alt(v) {
                        parts.push(expr);
                        alt_text.push(text);
                    }
                }
                "ability" => {
                    let alts: Vec<(String, String)> = v
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|o| {
                                    let m = o.as_object()?;
                                    let pairs: Vec<(String, i64, &str)> = m
                                        .iter()
                                        .filter_map(|(ab, n)| {
                                            Some((
                                                ab.clone(),
                                                n.as_i64()?,
                                                core_ids::ability_name(ab)?,
                                            ))
                                        })
                                        .collect();
                                    if pairs.is_empty() {
                                        return None;
                                    }
                                    let joined = pairs
                                        .iter()
                                        .map(|(a, n, _)| format!("[{a}:{n}]"))
                                        .collect::<Vec<_>>()
                                        .join("&&");
                                    let expr = if pairs.len() > 1 {
                                        format!("({joined})")
                                    } else {
                                        joined
                                    };
                                    let text = pairs
                                        .iter()
                                        .map(|(_, n, name)| format!("{name} {n} or higher"))
                                        .collect::<Vec<_>>()
                                        .join(" and ");
                                    Some((expr, text))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if !alts.is_empty() {
                        let joined = alts
                            .iter()
                            .map(|(e, _)| e.clone())
                            .collect::<Vec<_>>()
                            .join("||");
                        parts.push(if alts.len() > 1 {
                            format!("({joined})")
                        } else {
                            joined
                        });
                        alt_text.push(
                            alts.iter()
                                .map(|(_, t)| t.clone())
                                .collect::<Vec<_>>()
                                .join(" or "),
                        );
                    }
                }
                other => {
                    let text = describe(other, v);
                    if !text.is_empty() {
                        alt_text.push(text.clone());
                        unmapped.push(text);
                    }
                }
            }
        }
        if !alt_text.is_empty() {
            texts.push(alt_text.join(", "));
        }
        exprs.push((!parts.is_empty()).then(|| parts.join("&&")));
    }
    let requirements = if exprs.is_empty() || exprs.iter().any(Option::is_none) {
        String::new()
    } else {
        let list: Vec<String> = exprs.into_iter().flatten().collect();
        if list.len() == 1 {
            list[0].clone()
        } else {
            list.iter()
                .map(|e| format!("({e})"))
                .collect::<Vec<_>>()
                .join("||")
        }
    };
    unmapped.sort();
    unmapped.dedup();
    Prereq {
        text: texts.join(" or "),
        requirements,
        unmapped,
    }
}

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
    let Prereq {
        text,
        requirements,
        unmapped,
    } = prerequisites(&feat.prerequisite);
    el.prerequisite = text;
    el.requirements = requirements;
    for u in &unmapped {
        partial.push(format!("prerequisite not enforced: {u}"));
        note_unmapped(&mut el, "prerequisite", u);
    }

    let mut mech = Mech::new(sh, &feat.name);
    mech.ability(&feat.ability, AbilitySource::Feat);
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
                } else if k == "choose" {
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
    let source_display = sh.source_display(&ent.source);
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
    fn prerequisite_mapping() {
        let p = prerequisites(&[json!({"level": 4, "ability": [{"str": 13}, {"dex": 13}]})]);
        assert_eq!(p.requirements, "([str:13]||[dex:13])&&[character:4]");
        assert_eq!(
            p.text,
            "Strength 13 or higher or Dexterity 13 or higher, Level 4+"
        );
        assert!(p.unmapped.is_empty());
        let p = prerequisites(&[
            json!({"level": 4, "ability": [{"str": 13}]}),
            json!({"level": 4, "ability": [{"dex": 13}]}),
        ]);
        assert_eq!(
            p.requirements,
            "([str:13]&&[character:4])||([dex:13]&&[character:4])"
        );
        let p = prerequisites(&[
            json!({"spellcasting2020": true, "level": {"level": 5, "class": {"name": "Cleric"}}}),
        ]);
        assert_eq!(p.requirements, "[cleric:5]");
        assert_eq!(p.unmapped, vec!["Spellcasting"]);
        let p = prerequisites(&[
            json!({"race": [{"name": "elf", "displayEntry": "Elf"}]}),
            json!({"level": 3}),
        ]);
        assert_eq!(p.requirements, "");
        assert_eq!(p.text, "Race: Elf or Level 3+");
        let p = prerequisites(&[json!({"other": "Ability to speak {@b Draconic}"})]);
        assert_eq!(p.text, "Ability to speak Draconic");
        assert_eq!(prerequisites(&[]), Prereq::default());
        let p = prerequisites(&[json!({"feature": ["Fighting Style"]})]);
        assert_eq!(p.text, "Feature: Fighting Style");
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
        assert_eq!(el.supports, vec!["General"]);
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
        assert_eq!(el.supports, vec!["Origin"]);
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
        assert!(conv.partial[0].contains("Race: elf"));
        let bad = ent(json!({"name": "Bad", "source": "ZZZ", "entries": "text"}));
        assert!(convert(&sh, &mut st, &bad).elements.is_empty());
    }
}
