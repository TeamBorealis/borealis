//! Races, subraces and 2024 species.

use super::build::{grant, select, setter_with};
use super::mech::{AbilitySource, Mech};
use super::spell_blocks::{self, Carrier};
use super::spells_extra;
use super::{
    append_fluff, drain_renderer, new_element, note_unmapped, sheet_text, short_text, Conv, Ent,
    Shared, State,
};
use crate::ids::IdKind;
use crate::model::{self, Race};
use crate::srd::SrdKind;
use crate::text::{RefKind, Renderer};
use borealis_content::{Element, Rule, Ruleset};
use serde_json::Value;

/// Sections that are descriptive and stay in the race description.
const DESCRIPTIVE_SECTIONS: &[&str] = &[
    "age",
    "size",
    "languages",
    "language",
    "alignment",
    "ability score increase",
    "ability score increases",
    "creature type",
    "creature types",
];

/// Split top-level entries into descriptive text and named traits.
#[must_use]
pub fn split_traits(entries: &[Value]) -> (Vec<Value>, Vec<(String, Vec<Value>)>) {
    let mut description = Vec::new();
    let mut traits = Vec::new();
    for e in entries {
        let name = e.get("name").and_then(Value::as_str);
        let ty = e.get("type").and_then(Value::as_str).unwrap_or("entries");
        match name {
            Some(n)
                if ty == "entries"
                    && !DESCRIPTIVE_SECTIONS.contains(&n.to_lowercase().as_str()) =>
            {
                let body = e
                    .get("entries")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                traits.push((n.to_string(), body));
            }
            _ => description.push(e.clone()),
        }
    }
    (description, traits)
}

fn height_weight(e: &mut Element, hw: &Value) {
    if let (Some(h), Some(hm)) = (
        hw.get("baseHeight").and_then(Value::as_i64),
        hw.get("heightMod").and_then(Value::as_str),
    ) {
        e.setters.push(setter_with(
            "height",
            &format!("{}'{}\"", h / 12, h % 12),
            &[("modifier", hm)],
        ));
    }
    if let (Some(w), Some(wm)) = (
        hw.get("baseWeight").and_then(Value::as_i64),
        hw.get("weightMod").and_then(Value::as_str),
    ) {
        e.setters.push(setter_with(
            "weight",
            &format!("{w} lb."),
            &[("modifier", wm)],
        ));
    }
}

fn shown_subrace_name(own_name: &str) -> String {
    if own_name.is_empty() {
        "Standard".to_string()
    } else {
        own_name.to_string()
    }
}

/// The element id of a subrace (allocated once; the same call from the link pass and the
/// converter returns the same id).
pub fn subrace_id(st: &mut State, ent: &Ent) -> String {
    let parent_name = ent
        .raw
        .get("raceName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parent_source = ent
        .raw
        .get("raceSource")
        .and_then(Value::as_str)
        .unwrap_or(&ent.source);
    let own_name = ent
        .raw
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let shown = shown_subrace_name(own_name);
    st.ids.id(
        IdKind::Subrace,
        &ent.source,
        &[parent_name, &shown],
        &format!("{}|{}", race_key(parent_name, parent_source), shown),
    )
}

/// Parent race properties a subrace replaces: `overwrite: {prop: true}` or `prop: null`.
#[must_use]
pub fn replaced_props(raw: &Value) -> Vec<String> {
    let mut out: Vec<String> = raw
        .get("overwrite")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(_, v)| v.as_bool() == Some(true))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    if let Some(m) = raw.as_object() {
        out.extend(
            m.iter()
                .filter(|(_, v)| v.is_null())
                .map(|(k, _)| k.clone()),
        );
    }
    out.retain(|k| !METADATA_KEYS.contains(&k.as_str()));
    out.sort();
    out.dedup();
    out
}

/// Parent properties whose rules can be switched off while a replacing subrace is chosen.
const REPLACEABLE: &[&str] = &[
    "ability",
    "resist",
    "immune",
    "vulnerable",
    "conditionImmune",
    "languageProficiencies",
    "skillProficiencies",
    "toolProficiencies",
    "weaponProficiencies",
    "armorProficiencies",
    "skillToolLanguageProficiencies",
    "feats",
    "additionalSpells",
];

/// Bibliographic properties: removing them from a subrace changes nothing mechanical.
const METADATA_KEYS: &[&str] = &[
    "otherSources",
    "reprintedAs",
    "page",
    "hasFluff",
    "hasFluffImages",
    "srd",
    "srd52",
    "basicRules",
    "basicRules2024",
    "freeRules2024",
    "legacy",
    "traitTags",
];

fn race_key(name: &str, source: &str) -> String {
    format!("{}|{}", name.to_lowercase(), source.to_uppercase())
}

/// Convert one race, subrace or species.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent, is_subrace: bool) -> Conv {
    let race: Race = match model::parse(&ent.raw) {
        Ok(r) => r,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                if is_subrace { "subrace" } else { "race" },
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let source_display = sh.source_display(&ent.source);
    let is_2024 = ent.ruleset == Ruleset::Dnd2024;
    let parent_name = race.race_name.clone().unwrap_or_default();
    let parent_source = race
        .race_source
        .clone()
        .unwrap_or_else(|| ent.source.clone());
    let own_name = race.name.clone().unwrap_or_default();

    let (display_name, id, ty) = if is_subrace {
        let shown = shown_subrace_name(&own_name);
        let name = format!("{parent_name} ({shown})");
        let id = subrace_id(st, ent);
        (name, id, "Sub Race")
    } else if is_2024 {
        let id = st.ids.named(IdKind::Species, &ent.source, &own_name);
        (own_name.clone(), id, "Species")
    } else {
        let id = st.ids.named(IdKind::Race, &ent.source, &own_name);
        (own_name.clone(), id, "Race")
    };

    let mut el = new_element(&id, &display_name, ty, &source_display, ent.ruleset);
    if ty == "Species" {
        el.legacy_alias = Some("Race".into());
    }
    let mut mech = Mech::new(sh, &display_name);
    let mut partial: Vec<String> = Vec::new();

    // A parent race's rules for a property some subrace replaces apply only while no such
    // subrace is chosen (`!ID_SUBRACE...`).
    let replaced = if is_subrace {
        None
    } else {
        sh.race_replaced.get(&race_key(&own_name, &ent.source))
    };
    let gate_for = |props: &[&str]| -> Option<String> {
        let mut ids: Vec<&String> = props
            .iter()
            .filter_map(|p| replaced.and_then(|r| r.get(*p)))
            .flatten()
            .collect();
        ids.sort();
        ids.dedup();
        (!ids.is_empty()).then(|| {
            ids.iter()
                .map(|id| format!("!{id}"))
                .collect::<Vec<_>>()
                .join("&&")
        })
    };
    mech.set_gate(gate_for(&["ability"]).as_deref());
    mech.ability(&race.ability, AbilitySource::Race);
    mech.set_gate(None);
    mech.size(&race.size);
    mech.speed(race.speed.as_ref());
    mech.senses(race.darkvision, race.blindsight, &[]);
    mech.set_gate(gate_for(&["resist", "immune", "vulnerable", "conditionImmune"]).as_deref());
    mech.resistances(
        &race.resist,
        &race.immune,
        &race.vulnerable,
        &race.condition_immune,
    );
    mech.set_gate(gate_for(&["languageProficiencies"]).as_deref());
    mech.languages(&race.language_proficiencies);
    mech.set_gate(gate_for(&["skillProficiencies"]).as_deref());
    mech.skills(&race.skill_proficiencies);
    mech.set_gate(gate_for(&["toolProficiencies"]).as_deref());
    mech.tools(&race.tool_proficiencies);
    mech.set_gate(gate_for(&["weaponProficiencies"]).as_deref());
    mech.weapons(&race.weapon_proficiencies);
    mech.set_gate(gate_for(&["armorProficiencies"]).as_deref());
    mech.armor(&race.armor_proficiencies);
    mech.set_gate(gate_for(&["skillToolLanguageProficiencies"]).as_deref());
    mech.skill_tool_language(&race.skill_tool_language_proficiencies);
    mech.set_gate(gate_for(&["feats"]).as_deref());
    mech.feats(&race.feats);

    // Additional spells: unnamed groups apply directly, named groups become options.
    let spells = spells_extra::convert_inheriting(
        sh,
        &display_name,
        &race.additional_spells,
        true,
        &spells_extra::inherited_abilities(&race.ability),
    );
    let spell_gate = gate_for(&["additionalSpells"]);
    let placed = spell_blocks::place(
        &Carrier {
            id: &id,
            name: &display_name,
            source: &source_display,
            ruleset: ent.ruleset,
            free: spell_gate.is_none(),
        },
        &spells.casting,
        &spells.direct,
        true,
    );
    mech.set_gate(spell_gate.as_deref());
    for r in spells.direct.into_iter().chain(placed.rules) {
        mech.push(r);
    }
    mech.set_gate(None);
    el.spellcasting = placed.spellcasting;
    partial.extend(spells.partial);
    let mut extra_elements: Vec<Element> = placed.elements;
    if !spells.options.is_empty() {
        let option_ty = if is_2024 {
            "Species Trait Option"
        } else {
            "Racial Trait"
        };
        let owner_id = id.clone();
        let (select_ty, select_label) = (option_ty, if is_2024 { "Lineage" } else { "Variant" });
        for opt in spells.options {
            let opt_id = st.ids.id(
                IdKind::SpeciesOption,
                &ent.source,
                &[&display_name, &opt.name],
                &format!(
                    "{}|{}|{}",
                    race_key(&display_name, &ent.source),
                    "opt",
                    opt.name
                ),
            );
            let mut oe = new_element(&opt_id, &opt.name, option_ty, &source_display, ent.ruleset);
            oe.supports = vec![owner_id.clone()];
            oe.description = format!(
                "<p>{select_label} option of {}.</p>",
                crate::text::tags::esc(&display_name)
            );
            oe.sheet = Some(borealis_content::Sheet {
                display: Some(false),
                ..borealis_content::Sheet::default()
            });
            let helpers = spell_blocks::fill(&mut oe, opt.rules, &opt.casting, true);
            extra_elements.push(oe);
            extra_elements.extend(helpers);
        }
        let name = format!("{display_name} {select_label}");
        mech.push(Rule::Select(select(select_ty, &name, Some(&owner_id), 1)));
    }

    // Traits become their own elements.
    let (description_entries, traits) = split_traits(&race.entries);
    for (trait_name, body) in traits {
        let tid = st.ids.id(
            IdKind::RacialTrait,
            &ent.source,
            &[&display_name, &trait_name],
            &format!(
                "{}|{}|{}",
                race_key(&display_name, &ent.source),
                "trait",
                trait_name
            ),
        );
        let mut te = new_element(
            &tid,
            &trait_name,
            "Racial Trait",
            &source_display,
            ent.ruleset,
        );
        te.description = renderer.entries(&body);
        te.sheet = Some(sheet_text(&short_text(&Renderer::plain(&body), 300)));
        mech.push(grant(&tid, "Racial Trait"));
        extra_elements.push(te);
    }

    // Sub race choice.
    if !is_subrace
        && !is_2024
        && sh
            .races_with_subraces
            .contains(&race_key(&own_name, &ent.source))
    {
        mech.push(Rule::Select(select("Sub Race", "Subrace", Some(&id), 1)));
    }
    if is_subrace {
        el.supports = vec![sh
            .race_ids
            .get(&race_key(&parent_name, &parent_source))
            .cloned()
            .unwrap_or_else(|| {
                format!(
                    "ID_5ET_{}_RACE_{}",
                    crate::util::id_fragment(&parent_source),
                    crate::util::id_fragment(&parent_name)
                )
            })];
        let unsupported: Vec<String> = replaced_props(&ent.raw)
            .into_iter()
            .filter(|p| !REPLACEABLE.contains(&p.as_str()))
            .collect();
        if !unsupported.is_empty() {
            partial.push(format!(
                "subrace replaces the parent race's {} (not applied; the parent race's version stays)",
                unsupported.join(", ")
            ));
        }
    }

    if let Some(hw) = &race.height_and_weight {
        height_weight(&mut el, hw);
    }
    if is_2024 && !race.creature_types.is_empty() {
        el.data.insert(
            "creature_types".into(),
            Value::Array(race.creature_types.clone()),
        );
    }
    if !race.size.is_empty() {
        el.data.insert("size".into(), serde_json::json!(race.size));
    }

    let mut description = renderer.entries(&description_entries);
    append_fluff(sh, "race", ent, &mut description, &mut renderer);
    el.description = description;
    el.sheet = Some(borealis_content::Sheet {
        display: Some(false),
        ..borealis_content::Sheet::default()
    });
    el.rules = std::mem::take(&mut mech.rules);
    for (k, vs) in &mech.unmapped {
        note_unmapped(&mut el, k, &vs.join("; "));
    }
    partial.extend(std::mem::take(&mut mech.partial));
    el.maps_to = sh.maps_to(
        if is_subrace {
            SrdKind::Subrace
        } else {
            SrdKind::Race
        },
        ent.ruleset,
        &display_name,
        &race.reprinted_as,
        &|uid| sh.reprint(RefKind::Race, uid),
    );
    drain_renderer(st, &renderer, &ent.file, &display_name);
    partial.sort();
    partial.dedup();
    let mut elements = vec![el];
    elements.extend(extra_elements);
    Conv { elements, partial }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ent(raw: Value) -> Ent {
        let source = raw["source"].as_str().unwrap().to_string();
        Ent {
            name: raw
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: false,
            file: "races.json".into(),
            source,
            raw,
        }
    }

    #[test]
    fn splits_traits_from_descriptive_sections() {
        let (desc, traits) = split_traits(&[
            json!("intro"),
            json!({"type": "entries", "name": "Age", "entries": ["old"]}),
            json!({"type": "entries", "name": "Darkvision", "entries": ["see"]}),
            json!({"type": "list", "items": []}),
        ]);
        assert_eq!(desc.len(), 3);
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].0, "Darkvision");
    }

    #[test]
    fn dwarf_like_race() {
        let mut sh = Shared::default();
        sh.races_with_subraces.insert("dwarf|ZZZ".into());
        sh.race_ids
            .insert("dwarf|ZZZ".into(), "ID_5ET_ZZZ_RACE_DWARF".into());
        let mut st = State::default();
        let e = ent(json!({
            "name": "Dwarf", "source": "ZZZ", "size": ["M"], "speed": 25, "ability": [{"con": 2}],
            "darkvision": 60, "languageProficiencies": [{"common": true, "dwarvish": true}],
            "resist": ["poison"], "heightAndWeight": {"baseHeight": 56, "heightMod": "2d4", "baseWeight": 130, "weightMod": "2d6"},
            "entries": [
                {"type": "entries", "name": "Age", "entries": ["Lives long."]},
                {"type": "entries", "name": "Dwarven Resilience", "entries": ["Advantage vs poison."]}
            ]
        }));
        let conv = convert(&sh, &mut st, &e, false);
        assert!(conv.partial.is_empty(), "{:?}", conv.partial);
        let race = &conv.elements[0];
        assert_eq!(race.id, "ID_5ET_ZZZ_RACE_DWARF");
        assert_eq!(race.ty, "Race");
        assert_eq!(race.ruleset, Some(Ruleset::Dnd2014));
        let grants: Vec<_> = race
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) => Some(g.id.as_str()),
                _ => None,
            })
            .collect();
        assert!(grants.contains(&"ID_SIZE_MEDIUM"));
        assert!(grants.contains(&"ID_VISION_DARKVISION"));
        assert!(grants.contains(&"ID_LANGUAGE_COMMON"));
        assert!(grants.contains(&"ID_LANGUAGE_DWARVISH"));
        assert!(grants.contains(&"ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_POISON"));
        assert!(grants.contains(&"ID_5ET_ZZZ_RACIALTRAIT_DWARF_DWARVEN_RESILIENCE"));
        assert!(race
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Stat(s) if s.name == "constitution" && s.value == "2")));
        assert!(race.rules.iter().any(|r| matches!(r, Rule::Stat(s) if s.name == "innate speed" && s.value == "25" && s.bonus.as_deref() == Some("base"))));
        assert!(race.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Sub Race" && s.supports.as_deref() == Some("ID_5ET_ZZZ_RACE_DWARF"))));
        assert_eq!(race.setters[0].value, "4'8\"");
        assert_eq!(race.setters[0].attribute("modifier"), Some("2d4"));
        assert!(race.description.contains("Lives long."));
        assert!(!race.description.contains("Advantage vs poison"));
        assert_eq!(conv.elements[1].ty, "Racial Trait");
        assert!(conv.elements[1]
            .description
            .contains("Advantage vs poison."));
    }

    #[test]
    fn subrace_links_to_parent_and_flags_overwrite() {
        let mut sh = Shared::default();
        sh.race_ids
            .insert("dwarf|ZZZ".into(), "ID_5ET_ZZZ_RACE_DWARF".into());
        let mut st = State::default();
        let e = ent(
            json!({"name": "Hill", "source": "ZZZ", "raceName": "Dwarf", "raceSource": "ZZZ",
            "ability": [{"wis": 1}], "overwrite": {"languageProficiencies": true},
            "entries": [{"type": "entries", "name": "Toughness", "entries": ["+1 hp"]}]}),
        );
        let conv = convert(&sh, &mut st, &e, true);
        let sub = &conv.elements[0];
        assert_eq!(sub.ty, "Sub Race");
        assert_eq!(sub.name, "Dwarf (Hill)");
        assert_eq!(sub.supports, vec!["ID_5ET_ZZZ_RACE_DWARF"]);
        // languages can be replaced (the parent's are gated), so nothing is lost
        assert!(conv.partial.is_empty(), "{:?}", conv.partial);
        let unnamed = ent(
            json!({"source": "ZZZ", "raceName": "Human", "raceSource": "ZZZ", "ability": [{"str": 1}]}),
        );
        let conv = convert(&sh, &mut st, &unnamed, true);
        assert_eq!(conv.elements[0].name, "Human (Standard)");
        assert_eq!(conv.elements[0].supports, vec!["ID_5ET_ZZZ_RACE_HUMAN"]);
    }

    #[test]
    fn subrace_replacements_gate_the_parent_rules() {
        let mut st = State::default();
        let sub = ent(
            json!({"name": "Ashen", "source": "ZZZ", "raceName": "Scaled",
            "raceSource": "ZZZ", "ability": [{"int": 2}], "resist": null,
            "overwrite": {"ability": true, "traitTags": true, "speed": true},
            "otherSources": null}),
        );
        assert_eq!(replaced_props(&sub.raw), vec!["ability", "resist", "speed"]);
        let sub_id = subrace_id(&mut st, &sub);
        let mut sh = Shared::default();
        sh.race_ids
            .insert("scaled|ZZZ".into(), "ID_5ET_ZZZ_RACE_SCALED".into());
        for p in replaced_props(&sub.raw) {
            sh.race_replaced
                .entry("scaled|ZZZ".into())
                .or_default()
                .insert(p, vec![sub_id.clone()]);
        }
        let parent = ent(json!({"name": "Scaled", "source": "ZZZ", "speed": 30,
            "ability": [{"str": 2, "cha": 1}], "resist": ["fire"],
            "languageProficiencies": [{"common": true}]}));
        let conv = convert(&sh, &mut st, &parent, false);
        let gate = format!("!{sub_id}");
        let gated = |r: &Rule| match r {
            Rule::Stat(s) => s.requirements.as_deref() == Some(gate.as_str()),
            Rule::Grant(g) => g.requirements.as_deref() == Some(gate.as_str()),
            _ => false,
        };
        let rules = &conv.elements[0].rules;
        let stats: Vec<&Rule> = rules
            .iter()
            .filter(|r| matches!(r, Rule::Stat(s) if s.name == "strength" || s.name == "charisma"))
            .collect();
        assert_eq!(stats.len(), 2);
        assert!(stats.iter().all(|r| gated(r)), "{stats:?}");
        assert!(rules.iter().any(
            |r| matches!(r, Rule::Grant(g) if g.ty.as_deref() == Some("Condition")) && gated(r)
        ));
        assert!(rules.iter().any(
            |r| matches!(r, Rule::Grant(g) if g.ty.as_deref() == Some("Language")) && !gated(r)
        ));
        // the subrace itself converts, and only the unsupported replacement is reported
        let conv = convert(&sh, &mut st, &sub, true);
        assert_eq!(conv.elements[0].id, sub_id);
        assert_eq!(conv.partial.len(), 1, "{:?}", conv.partial);
        assert!(conv.partial[0].contains("speed"));
    }

    #[test]
    fn species_with_lineage_options() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(json!({
            "name": "Elf", "source": "XPHB", "edition": "one", "creatureTypes": ["humanoid"], "size": ["M"], "speed": 30, "darkvision": 60,
            "skillProficiencies": [{"choose": {"from": ["insight", "perception", "survival"]}}],
            "additionalSpells": [
                {"name": "Drow", "ability": {"choose": ["int", "wis", "cha"]}, "known": {"1": ["dancing lights|xphb#c"]}, "innate": {"3": {"daily": {"1": ["faerie fire|xphb"]}}}},
                {"name": "Wood Elf", "ability": {"choose": ["int", "wis", "cha"]}, "known": {"1": ["druidcraft|xphb#c"]}}
            ],
            "entries": [{"type": "entries", "name": "Elven Lineage", "entries": ["Choose a lineage."]}]
        }));
        let conv = convert(&sh, &mut st, &e, false);
        let sp = &conv.elements[0];
        assert_eq!(sp.ty, "Species");
        assert_eq!(sp.legacy_alias.as_deref(), Some("Race"));
        assert_eq!(sp.ruleset, Some(Ruleset::Dnd2024));
        assert_eq!(sp.data["creature_types"], json!(["humanoid"]));
        assert!(sp
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Select(s) if s.ty == "Species Trait Option")));
        assert!(sp.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Proficiency" && s.supports.as_deref().is_some_and(|x| x.contains("INSIGHT|")))));
        let opts: Vec<_> = conv
            .elements
            .iter()
            .filter(|x| x.ty == "Species Trait Option")
            .collect();
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0].name, "Drow");
        assert_eq!(opts[0].supports, vec![sp.id.clone()]);
        assert_eq!(opts[0].id, "ID_5ET_XPHB_SPECIESOPT_ELF_DROW");
        assert!(!sp
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Select(s) if s.ty == "Sub Race")));
    }

    #[test]
    fn bad_shapes_are_skipped() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(json!({"name": "Bad", "source": "ZZZ", "size": "M"}));
        let conv = convert(&sh, &mut st, &e, false);
        assert!(conv.elements.is_empty());
        let rep = st.report.build();
        assert_eq!(rep.skipped.len(), 1);
        assert!(rep.skipped[0].reason.contains("invalid shape"));
    }
}
