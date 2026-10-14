//! Classes.

use super::build::{grant, grant_at, select, set_level};
use super::caster::{caster, class_table, CasterInput};
use super::feature::{class_feature_key, feature_id, parse_class_feature_uid};
use super::mech::Mech;
use super::optfeat::{feat_selects, optional_feature_selects};
use super::spell_choices::known_without_extra_choices;
use super::{append_fluff, drain_renderer, new_element, note_unmapped, Conv, Ent, Shared, State};
use crate::core_ids;
use crate::ids::IdKind;
use crate::model::{self, Class};
use crate::srd::SrdKind;
use crate::text::tags::esc;
use crate::text::{strip_tags, RefKind, Renderer};
use borealis_content::{Multiclass, Rule, Setter};
use serde_json::{json, Value};

/// Name of the thing inside a `{@item name|source|display}` tag, or the text itself.
fn tag_arg0(text: &str) -> String {
    if let Some(rest) = text.strip_prefix("{@") {
        if let Some((_, body)) = rest.split_once(' ') {
            return body
                .split(['|', '}'])
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
        }
    }
    strip_tags(text).trim().to_string()
}

fn singular(name: &str) -> String {
    name.strip_suffix('s').unwrap_or(name).to_string()
}

fn prof_strings(v: &[Value]) -> Vec<String> {
    v.iter()
        .filter_map(|x| match x {
            Value::String(s) => Some(s.clone()),
            other => other
                .get("proficiency")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect()
}

/// Apply the proficiencies of a `startingProficiencies` / `proficienciesGained` object.
pub fn apply_proficiencies(mech: &mut Mech<'_>, p: &Value) {
    let arr = |key: &str| -> Vec<Value> {
        p.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let skills = arr("skills");
    mech.skills(&skills);

    let armor = arr("armorProficiencies");
    if armor.is_empty() {
        let strings = prof_strings(&arr("armor"));
        let objs: Vec<Value> = strings
            .iter()
            .map(|s| json!({ tag_arg0(s): true }))
            .collect();
        mech.armor(&objs);
    } else {
        mech.armor(&armor);
    }

    let weapons = arr("weaponProficiencies");
    if weapons.is_empty() {
        let mut obj = serde_json::Map::new();
        for s in prof_strings(&arr("weapons")) {
            let name = tag_arg0(&s).to_lowercase();
            let name = if core_ids::weapon_prof_id(&name).is_some() {
                name
            } else {
                singular(&name)
            };
            obj.insert(name, Value::Bool(true));
        }
        if !obj.is_empty() {
            mech.weapons(&[Value::Object(obj)]);
        }
    } else {
        mech.weapons(&weapons);
    }

    let tools = arr("toolProficiencies");
    if tools.is_empty() {
        let mut obj = serde_json::Map::new();
        for s in prof_strings(&arr("tools")) {
            let name = tag_arg0(&s).to_lowercase();
            obj.insert(name, Value::Bool(true));
        }
        if !obj.is_empty() {
            mech.tools(&[Value::Object(obj)]);
        }
    } else {
        mech.tools(&tools);
    }
}

fn join_names(abbrs: &[&str]) -> String {
    abbrs
        .iter()
        .filter_map(|a| core_ids::ability_name(a))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// Requirements expression and display text of the multiclass prerequisite.
#[must_use]
pub fn multiclass_requirements(class: &Class) -> (String, String) {
    let mc = class.multiclassing.as_ref();
    let req = mc.and_then(|m| m.get("requirements"));
    let mut and_parts: Vec<(String, i64)> = Vec::new();
    let mut or_parts: Vec<(String, i64)> = Vec::new();
    if let Some(Value::Object(o)) = req {
        for (k, v) in o {
            match (k.as_str(), v) {
                ("or", Value::Array(alts)) => {
                    for alt in alts {
                        if let Some(m) = alt.as_object() {
                            for (ak, av) in m {
                                if let Some(n) = av.as_i64() {
                                    or_parts.push((ak.clone(), n));
                                }
                            }
                        }
                    }
                }
                (ab, v) => {
                    if let (Some(n), true) = (v.as_i64(), core_ids::ability_name(ab).is_some()) {
                        and_parts.push((ab.to_string(), n));
                    }
                }
            }
        }
    } else if !class.primary_ability.is_empty() {
        // 2024: 13 in the primary ability of the class.
        for alt in &class.primary_ability {
            if let Some(m) = alt.as_object() {
                let keys: Vec<(String, i64)> = m.keys().map(|k| (k.clone(), 13)).collect();
                if keys.len() == 1 {
                    or_parts.push(keys[0].clone());
                } else {
                    and_parts.extend(keys);
                }
            }
        }
    }
    let order = |a: &str| {
        core_ids::ABILITIES
            .iter()
            .position(|(x, _)| *x == a)
            .unwrap_or(6)
    };
    and_parts.sort_by_key(|(a, _)| order(a));
    or_parts.sort_by_key(|(a, _)| order(a));
    let mut expr = Vec::new();
    let mut text = Vec::new();
    for (a, n) in &and_parts {
        expr.push(format!("[{a}:{n}]"));
        text.push(format!("{} {n}", join_names(&[a])));
    }
    if !or_parts.is_empty() {
        expr.push(format!(
            "({})",
            or_parts
                .iter()
                .map(|(a, n)| format!("[{a}:{n}]"))
                .collect::<Vec<_>>()
                .join("||")
        ));
        text.push(
            or_parts
                .iter()
                .map(|(a, n)| format!("{} {n}", join_names(&[a])))
                .collect::<Vec<_>>()
                .join(" or "),
        );
    }
    (expr.join("&&"), text.join(" and "))
}

fn primary_ability_text(class: &Class) -> String {
    class
        .primary_ability
        .iter()
        .filter_map(|alt| {
            let keys: Vec<&str> = alt.as_object()?.keys().map(String::as_str).collect();
            let names: Vec<&str> = keys
                .iter()
                .filter_map(|k| core_ids::ability_name(k))
                .collect();
            (!names.is_empty()).then(|| names.join(" and "))
        })
        .collect::<Vec<_>>()
        .join(" or ")
}

fn proficiency_summary(p: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(a) = p.get("armor").and_then(Value::as_array) {
        let names: Vec<String> = a
            .iter()
            .filter_map(|x| x.as_str())
            .map(|s| {
                if s == "shield" {
                    "shields".to_string()
                } else {
                    format!("{s} armor")
                }
            })
            .collect();
        parts.extend(names);
    }
    if let Some(w) = p.get("weapons").and_then(Value::as_array) {
        for s in prof_strings(w) {
            let t = strip_tags(&s);
            parts.push(if t == "simple" || t == "martial" {
                format!("{t} weapons")
            } else {
                t
            });
        }
    }
    if let Some(t) = p.get("tools").and_then(Value::as_array) {
        parts.extend(prof_strings(t).iter().map(|s| strip_tags(s)));
    }
    if let Some(Value::Array(skills)) = p.get("skills") {
        for s in skills {
            if let Some(c) = s.get("choose") {
                let count = c.get("count").and_then(Value::as_u64).unwrap_or(1);
                parts.push(format!(
                    "{count} skill{} of your choice",
                    if count == 1 { "" } else { "s" }
                ));
            }
        }
    }
    parts.join(", ")
}

/// Convert one class.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let class: Class = match model::parse(&ent.raw) {
        Ok(c) => c,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "class",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let Some(hd) = class.hd.filter(|h| h.faces > 0) else {
        st.report
            .skipped(&ent.file, "class", &ent.label(), "class has no hit die");
        return Conv::default();
    };
    let is_2024 = ent.ruleset == borealis_content::Ruleset::Dnd2024;
    let class_key = format!(
        "{}|{}",
        class.name.to_lowercase(),
        ent.source.to_uppercase()
    );
    let id = st.ids.named(IdKind::Class, &ent.source, &class.name);
    let mc_id = st
        .ids
        .id(IdKind::Multiclass, &ent.source, &[&class.name], &class.name);
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let mut el = new_element(
        &id,
        &class.name,
        "Class",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let mut partial: Vec<String> = Vec::new();
    let mut mech = Mech::new(sh, &class.name);

    // Starting proficiencies apply only to the first class.
    let not_multiclass = format!("!{mc_id}");
    mech.set_gate(Some(&not_multiclass));
    mech.saving_throws(&class.proficiency);
    if let Some(p) = &class.starting_proficiencies {
        apply_proficiencies(&mut mech, p);
    }
    mech.set_gate(None);

    // Class features.
    let mut subclass_levels: Vec<u32> = Vec::new();
    for item in &class.class_features {
        let (uid, gains_subclass) = match item {
            Value::String(s) => (s.as_str(), false),
            other => (
                other
                    .get("classFeature")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                other.get("gainSubclassFeature") == Some(&Value::Bool(true)),
            ),
        };
        let Some(r) = parse_class_feature_uid(uid) else {
            partial.push(format!("class feature reference {uid:?} is malformed"));
            continue;
        };
        if gains_subclass {
            subclass_levels.push(r.level);
        }
        let key = class_feature_key(&r.name, &r.class, &r.class_source, r.level, &r.source);
        if sh.class_feature_keys.contains_key(&key) {
            let fid = feature_id(st, &r, false);
            mech.push(grant_at(&fid, "Class Feature", r.level));
        } else if !gains_subclass {
            partial.push(format!(
                "class feature {:?} ({}) not found in the data",
                r.name, r.source
            ));
        }
    }
    if let Some(level) = subclass_levels.iter().min() {
        let title = class
            .subclass_title
            .clone()
            .unwrap_or_else(|| "Subclass".to_string());
        let mut rule = Rule::Select(select("Archetype", &title, Some(&id), 1));
        if let Err(e) = set_level(&mut rule, *level) {
            mech.partial.push(e);
        }
        mech.push(rule);
    }
    for r in optional_feature_selects(&class.optionalfeature_progression, &mut partial) {
        mech.push(r);
    }
    for r in feat_selects(&class.feat_progression, &mut partial) {
        mech.push(r);
    }

    // Spellcasting and the class table.
    let known =
        known_without_extra_choices(&class.spells_known_progression, &class.additional_spells);
    let c = caster(&CasterInput {
        owner: &class.name,
        class_name: &class.name,
        list_tag: &class.name,
        ability: class.spellcasting_ability.as_deref(),
        progression: class.caster_progression.as_deref(),
        groups: &class.class_table_groups,
        cantrips: &class.cantrip_progression,
        known: &known,
        known_fixed: &class.spells_known_progression_fixed,
        prepared_formula: class.prepared_spells.as_deref(),
        prepared_progression: &class.prepared_spells_progression,
        book_allow_lower: class
            .spells_known_progression_fixed_allow_lower_level
            .unwrap_or(false),
        by_level: class.spells_known_progression_fixed_by_level.as_ref(),
        prepared_change: class.prepared_spells_change.as_deref(),
        schools: None,
        is_2024,
    });
    for r in c.rules {
        mech.push(r);
    }
    partial.extend(c.partial);
    el.spellcasting = c.spellcasting;
    let table = class_table(&class.name, &class.class_table_groups);
    for r in table.rules {
        mech.push(r);
    }
    if let Some(t) = table.data {
        el.data.insert("class_table".into(), t);
    }
    if let Some(p) = super::feature_rules::table_progression(&class.class_table_groups) {
        el.data.insert("progression".into(), p);
    }

    // Class level additional spells (Artificer's expanded lists and similar).
    let (spell_rules, spell_partial, spell_helpers) = spells_extra_class(sh, &class, &el);
    for r in spell_rules {
        mech.push(r);
    }
    partial.extend(spell_partial);

    // Multiclass node.
    let (requirements, prerequisite) = multiclass_requirements(&class);
    let mut mc_mech = Mech::new(sh, &class.name);
    mc_mech.push(grant("ID_INTERNAL_GRANT_MULTICLASS", "Grants"));
    let mut mc_text = String::new();
    if let Some(gained) = class
        .multiclassing
        .as_ref()
        .and_then(|m| m.get("proficienciesGained"))
    {
        apply_proficiencies(&mut mc_mech, gained);
        mc_text = proficiency_summary(gained);
    }
    let mc_description = class
        .multiclassing
        .as_ref()
        .and_then(|m| m.get("entries"))
        .and_then(Value::as_array)
        .map(|e| renderer.entries(e))
        .unwrap_or_default();
    partial.extend(std::mem::take(&mut mc_mech.partial));
    let mut mc = Multiclass {
        id: mc_id,
        prerequisite,
        requirements,
        description: mc_description,
        rules: std::mem::take(&mut mc_mech.rules),
        ..Multiclass::default()
    };
    mc.setters.push(Setter::new(
        "multiclass proficiencies",
        if mc_text.is_empty() {
            "\u{2014}".to_string()
        } else {
            mc_text
        },
    ));
    el.multiclass = Some(mc);

    el.setters = vec![
        Setter::new("short", short_summary(&class, hd.faces)),
        Setter::new("hd", format!("d{}", hd.faces)),
    ];
    el.rules = std::mem::take(&mut mech.rules);
    for (k, vs) in &mech.unmapped {
        note_unmapped(&mut el, k, &vs.join("; "));
    }
    partial.extend(std::mem::take(&mut mech.partial));

    // Data kept for the UI and for tooling.
    el.data.insert("hit_die".into(), json!(hd.faces));
    if let Some(p) = &class.caster_progression {
        el.data.insert("caster_progression".into(), json!(p));
    }
    if let Some(f) = &class.prepared_spells {
        el.data.insert("prepared_formula".into(), json!(f));
    }
    if let Some(t) = &class.subclass_title {
        el.data.insert("subclass_title".into(), json!(t));
    }
    if !class.primary_ability.is_empty() {
        el.data.insert(
            "primary_ability".into(),
            Value::Array(class.primary_ability.clone()),
        );
    }
    if let Some(se) = &class.starting_equipment {
        el.data.insert("starting_equipment".into(), se.clone());
    }

    let mut description = describe(&class, hd.faces, &renderer);
    append_fluff(sh, "class", ent, &mut description, &mut renderer);
    el.description = description;
    el.sheet = Some(borealis_content::Sheet {
        display: Some(false),
        ..borealis_content::Sheet::default()
    });
    el.maps_to = sh.maps_to(
        SrdKind::Class,
        ent.ruleset,
        &class.name,
        &class.reprinted_as,
        &|uid| sh.reprint(RefKind::Class, uid),
    );
    let _ = class_key;
    drain_renderer(st, &renderer, &ent.file, &class.name);
    partial.sort();
    partial.dedup();
    let mut elements = vec![el];
    elements.extend(spell_helpers);
    Conv { elements, partial }
}

/// Class-level additional spells (Magical Secrets, always-prepared signature spells, expanded
/// lists): class spells, so they use the class's own block; extensions go on helpers.
fn spells_extra_class(
    sh: &Shared,
    class: &Class,
    el: &borealis_content::Element,
) -> (Vec<Rule>, Vec<String>, Vec<borealis_content::Element>) {
    if class.additional_spells.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let own_block = el.spellcasting.is_none();
    let r = super::spells_extra::convert(sh, &class.name, &class.additional_spells, own_block);
    let mut partial = r.partial;
    if !r.options.is_empty() {
        partial.push("alternative additional spell groups of a class were not converted".into());
    }
    let carrier = super::spell_blocks::Carrier {
        id: &el.id,
        name: &el.name,
        source: &el.source,
        ruleset: el.ruleset.unwrap_or(borealis_content::Ruleset::Dnd2014),
        free: false,
    };
    let placed = super::spell_blocks::place(&carrier, &r.casting, &r.direct, own_block);
    let mut rules = r.direct;
    rules.extend(placed.rules);
    (rules, partial, placed.elements)
}

fn short_summary(class: &Class, faces: u32) -> String {
    let saves: Vec<&str> = class
        .proficiency
        .iter()
        .filter_map(|a| core_ids::ability_name(a))
        .collect();
    format!("Hit die d{faces}; saving throws {}", saves.join(", "))
}

fn describe(class: &Class, faces: u32, renderer: &Renderer<'_>) -> String {
    let mut html = format!("<p><strong>Hit Die:</strong> d{faces}</p>");
    let primary = primary_ability_text(class);
    if !primary.is_empty() {
        html.push_str(&format!(
            "<p><strong>Primary Ability:</strong> {}</p>",
            esc(&primary)
        ));
    }
    let saves: Vec<&str> = class
        .proficiency
        .iter()
        .filter_map(|a| core_ids::ability_name(a))
        .collect();
    if !saves.is_empty() {
        html.push_str(&format!(
            "<p><strong>Saving Throw Proficiencies:</strong> {}</p>",
            saves.join(", ")
        ));
    }
    if let Some(p) = &class.starting_proficiencies {
        let s = proficiency_summary(p);
        if !s.is_empty() {
            html.push_str(&format!(
                "<p><strong>Starting Proficiencies:</strong> {}</p>",
                esc(&s)
            ));
        }
    }
    if let Some(se) = &class.starting_equipment {
        let lines: Vec<String> = se
            .get("default")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(|s| renderer.inline(s))
                    .collect()
            })
            .or_else(|| {
                se.get("entries").and_then(Value::as_array).map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(|s| renderer.inline(s))
                        .collect()
                })
            })
            .unwrap_or_default();
        if !lines.is_empty() {
            html.push_str("<p><strong>Starting Equipment:</strong></p><ul>");
            for l in lines {
                html.push_str(&format!("<li>{l}</li>"));
            }
            html.push_str("</ul>");
        }
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::feature::key_of_feature;
    use crate::model::Feature;
    use borealis_content::Ruleset;

    fn ent(raw: Value) -> Ent {
        Ent {
            name: raw["name"].as_str().unwrap().to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: true,
            file: "class/class-zzz.json".into(),
            raw,
        }
    }

    fn with_features(raw_features: &[Value]) -> Shared {
        let mut sh = Shared::default();
        for raw in raw_features {
            let f: Feature = model::parse(raw).unwrap();
            sh.class_feature_keys
                .insert(key_of_feature(&f, false), sh.class_features.len());
            sh.class_features.push(ent(raw.clone()));
        }
        sh
    }

    fn fighter_json() -> Value {
        json!({
            "name": "Fighter", "source": "ZZZ", "edition": "classic", "hd": {"number": 1, "faces": 10},
            "proficiency": ["str", "con"],
            "startingProficiencies": {
                "skills": [{"choose": {"from": ["acrobatics", "athletics", "history"], "count": 2}}],
                "weapons": ["simple", "martial"], "armor": ["light", "medium", "heavy", "shield"]},
            "startingEquipment": {"default": ["(a) {@item chain mail|phb} or (b) leather armor"]},
            "multiclassing": {"requirements": {"or": [{"str": 13, "dex": 13}]},
                "proficienciesGained": {"weapons": ["simple", "martial"], "armor": ["light", "medium", "shield"]}},
            "optionalfeatureProgression": [{"name": "Fighting Style", "featureType": ["FS:F"], "progression": {"1": 1}}],
            "classFeatures": ["Second Wind|Fighter|ZZZ|1", {"classFeature": "Martial Archetype|Fighter|ZZZ|3", "gainSubclassFeature": true},
                {"classFeature": "Martial Archetype feature|Fighter|ZZZ|7", "gainSubclassFeature": true},
                "Ability Score Improvement|Fighter|ZZZ|4", "Missing Feature|Fighter|ZZZ|5"],
            "subclassTitle": "Martial Archetype"
        })
    }

    fn fighter_features() -> Vec<Value> {
        let f = |name: &str, level: u32| json!({"name": name, "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ", "level": level, "entries": ["x"]});
        vec![
            f("Second Wind", 1),
            f("Martial Archetype", 3),
            f("Ability Score Improvement", 4),
        ]
    }

    #[test]
    fn fighter_like_class() {
        let sh = with_features(&fighter_features());
        let mut st = State::default();
        let conv = convert(&sh, &mut st, &ent(fighter_json()));
        let el = &conv.elements[0];
        assert_eq!(el.id, "ID_5ET_ZZZ_CLASS_FIGHTER");
        assert_eq!(el.ty, "Class");
        assert_eq!(el.setter_value("hd"), Some("d10"));
        assert_eq!(
            el.setter_value("short"),
            Some("Hit die d10; saving throws Strength, Constitution")
        );
        let mc = el.multiclass.as_ref().unwrap();
        assert_eq!(mc.id, "ID_5ET_ZZZ_MULTICLASS_FIGHTER");
        assert_eq!(mc.requirements, "([str:13]||[dex:13])");
        assert_eq!(mc.prerequisite, "Strength 13 or Dexterity 13");
        assert!(mc
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_INTERNAL_GRANT_MULTICLASS")));
        assert!(mc.rules.iter().any(
            |r| matches!(r, Rule::Grant(g) if g.id == "ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELDS")
        ));
        assert!(!mc.rules.iter().any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR")));
        assert_eq!(
            mc.setters[0].value,
            "light armor, medium armor, shields, simple weapons, martial weapons"
        );
        // starting grants are gated by the multiclass id
        let save = el
            .rules
            .iter()
            .find_map(|r| match r {
                Rule::Grant(g) if g.id.contains("SAVINGTHROW_STRENGTH") => Some(g),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            save.requirements.as_deref(),
            Some("!ID_5ET_ZZZ_MULTICLASS_FIGHTER")
        );
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR" && g.requirements.is_some())));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Proficiency" && s.requirements.is_some() && s.number == Some(2))));
        // features
        let grants: Vec<_> = el
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) if g.ty.as_deref() == Some("Class Feature") => {
                    Some((g.id.as_str(), g.level))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            grants,
            vec![
                ("ID_5ET_ZZZ_CLASSFEATURE_FIGHTER_L1_SECOND_WIND", None),
                (
                    "ID_5ET_ZZZ_CLASSFEATURE_FIGHTER_L3_MARTIAL_ARCHETYPE",
                    Some(3)
                ),
                (
                    "ID_5ET_ZZZ_CLASSFEATURE_FIGHTER_L4_ABILITY_SCORE_IMPROVEMENT",
                    Some(4)
                ),
            ]
        );
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Archetype" && s.level == Some(3) && s.name == "Martial Archetype" && s.supports.as_deref() == Some("ID_5ET_ZZZ_CLASS_FIGHTER"))));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Class Feature" && s.name == "Fighting Style" && s.supports.as_deref() == Some("OF_FS-F"))));
        // the placeholder for later subclass features is silent, the truly missing one is reported
        assert_eq!(conv.partial.len(), 1, "{:?}", conv.partial);
        assert!(conv.partial[0].contains("Missing Feature"));
        assert!(el.description.contains("Hit Die:</strong> d10"));
        assert!(
            el.description
                .contains("<li>(a) chain mail or (b) leather armor</li>")
                || el.description.contains("chain mail")
        );
        assert_eq!(el.data["hit_die"], 10);
        assert_eq!(el.ruleset, Some(Ruleset::Dnd2014));
        assert!(el.spellcasting.is_none());
        for required in borealis_content::validate::required_setters("Class") {
            assert!(el.setter(required).is_some());
        }
    }

    #[test]
    fn caster_class_2024() {
        let sh = Shared::default();
        let mut st = State::default();
        let raw = json!({
            "name": "Wizard", "source": "XPHB", "edition": "one", "hd": {"number": 1, "faces": 6}, "proficiency": ["int", "wis"],
            "primaryAbility": [{"int": true}], "casterProgression": "full", "spellcastingAbility": "int",
            "cantripProgression": [3, 3], "preparedSpellsProgression": [4, 5], "spellsKnownProgressionFixed": [6, 2],
            "spellsKnownProgressionFixedAllowLowerLevel": true, "preparedSpellsChange": "restLong",
            "featProgression": [{"name": "Epic Boon", "category": ["EB"], "progression": {"19": 1}}],
            "classTableGroups": [{"colLabels": ["Cantrips", "Prepared Spells"], "rows": [[3, 4], [3, 5]]},
                {"title": "Spell Slots per Spell Level", "colLabels": ["1st"], "rowsSpellProgression": [[2,0,0,0,0,0,0,0,0],[3,0,0,0,0,0,0,0,0]]}],
            "multiclassing": {"proficienciesGained": {"weapons": ["simple"]}},
            "classFeatures": []});
        let conv = convert(&sh, &mut st, &ent(raw));
        let el = &conv.elements[0];
        assert!(conv.partial.is_empty(), "{:?}", conv.partial);
        assert_eq!(el.ruleset, Some(Ruleset::Dnd2024));
        let mc = el.multiclass.as_ref().unwrap();
        assert_eq!(mc.requirements, "([int:13])", "single primary ability");
        let sc = el.spellcasting.as_ref().unwrap();
        assert_eq!(sc.ability.as_deref(), Some("Intelligence"));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Stat(s) if s.name == "wizard:spellcasting:slots:1" && s.value == "2")));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Stat(s) if s.name == "wizard:spellcasting:prepare" && s.value == "4")));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Feat" && s.level == Some(19) && s.supports.as_deref() == Some("Epic Boon Feat"))));
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id.ends_with("SLOTS_FULL"))));
        assert!(el.data.contains_key("class_table"));
        assert_eq!(el.data["primary_ability"], json!([{"int": true}]));
        // spell choices: cantrips and spellbook pages as the bundled 2024 Wizard
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.name == "Cantrip (Wizard 1)" && s.number == Some(3) && s.allow_replace == Some(true))));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.name == "Spellbook (Wizard 2)" && s.number == Some(2) && s.level == Some(2))));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Custom(c) if c.kind == "spellbook" && c.fields["initial_spells"] == 6)));
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Custom(c) if c.kind == "prepared_spell_change" && c.fields["count"] == "any")));
        assert_eq!(sc.allow_replace, Some(true));
    }

    #[test]
    fn multiclass_requirement_forms() {
        let mk = |v: Value| -> Class { model::parse(&v).unwrap() };
        let (e, t) = multiclass_requirements(&mk(
            json!({"multiclassing": {"requirements": {"dex": 13, "wis": 13}}}),
        ));
        assert_eq!(
            (e.as_str(), t.as_str()),
            ("[dex:13]&&[wis:13]", "Dexterity 13 and Wisdom 13")
        );
        let (e, _) = multiclass_requirements(&mk(
            json!({"primaryAbility": [{"str": true}, {"dex": true}]}),
        ));
        assert_eq!(e, "([str:13]||[dex:13])");
        let (e, t) =
            multiclass_requirements(&mk(json!({"primaryAbility": [{"str": true, "cha": true}]})));
        assert_eq!(e, "[str:13]&&[cha:13]");
        assert_eq!(t, "Strength 13 and Charisma 13");
        let (e, t) = multiclass_requirements(&mk(json!({})));
        assert!(e.is_empty() && t.is_empty());
    }

    #[test]
    fn invalid_classes_and_helpers() {
        let sh = Shared::default();
        let mut st = State::default();
        assert!(convert(
            &sh,
            &mut st,
            &ent(json!({"name": "NoDie", "source": "ZZZ"}))
        )
        .elements
        .is_empty());
        assert!(convert(
            &sh,
            &mut st,
            &ent(json!({"name": "Bad", "source": "ZZZ", "hd": "d8"}))
        )
        .elements
        .is_empty());
        assert_eq!(st.report.build().skipped.len(), 2);
        assert_eq!(tag_arg0("{@item dagger|phb|daggers}"), "dagger");
        assert_eq!(tag_arg0("simple"), "simple");
        assert_eq!(singular("quarterstaffs"), "quarterstaff");
        let sh = Shared::default();
        let mut m = Mech::new(&sh, "C");
        apply_proficiencies(
            &mut m,
            &json!({"weapons": ["{@item dagger|phb|daggers}", "{@item light crossbow|phb|light crossbows}", "hand crossbows"],
            "tools": ["{@item thieves' tools|phb}", "three musical instruments of your choice"],
            "armor": [{"proficiency": "light", "full": "light armor"}]}),
        );
        let ids: Vec<String> = m
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) => Some(g.id.clone()),
                _ => None,
            })
            .collect();
        assert!(ids.contains(&"ID_PROFICIENCY_WEAPON_PROFICIENCY_DAGGER".to_string()));
        assert!(ids.contains(&"ID_PROFICIENCY_WEAPON_PROFICIENCY_CROSSBOW_LIGHT".to_string()));
        assert!(ids.contains(&"ID_PROFICIENCY_WEAPON_PROFICIENCY_CROSSBOW_HAND".to_string()));
        assert!(ids.contains(&"ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS".to_string()));
        assert!(ids.contains(&"ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR".to_string()));
        assert_eq!(m.partial.len(), 1);
    }
}
