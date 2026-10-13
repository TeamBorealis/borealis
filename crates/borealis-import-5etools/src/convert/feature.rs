//! Class features, subclass features and subclasses.

use super::build::{grant_at, select, set_level};
use super::caster::{caster, class_table, CasterInput};
use super::mech::Mech;
use super::optfeat::{feat_selects, optional_feature_selects};
use super::spell_blocks;
use super::spell_choices::school_rule;
use super::spells_extra;
use super::{
    append_fluff, drain_renderer, new_element, note_unmapped, sheet_text, short_text, Conv, Ent,
    Shared, State,
};
use crate::ids::IdKind;
use crate::model::{self, Feature, Subclass};
use crate::srd::SrdKind;
use crate::text::{RefKind, Renderer};
use crate::util::supports_token;
use borealis_content::{Rule, Spellcasting};
use serde_json::Value;

/// A parsed feature reference (`name|class|classSource|level|source`, or the subclass form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureRef {
    /// Feature name.
    pub name: String,
    /// Class name.
    pub class: String,
    /// Class source.
    pub class_source: String,
    /// Subclass short name (subclass features).
    pub sub_short: Option<String>,
    /// Subclass source (subclass features).
    pub sub_source: Option<String>,
    /// Level.
    pub level: u32,
    /// Source of the feature.
    pub source: String,
}

fn or_default<'a>(part: Option<&'a str>, default: &'a str) -> &'a str {
    part.map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(default)
}

/// Parse `name|class|classSource|level|source`.
#[must_use]
pub fn parse_class_feature_uid(uid: &str) -> Option<FeatureRef> {
    let p: Vec<&str> = uid.split('|').collect();
    let name = p.first()?.trim();
    let class = p.get(1)?.trim();
    let class_source = or_default(p.get(2).copied(), "PHB").to_string();
    let level = p.get(3)?.trim().parse().ok()?;
    let source = or_default(p.get(4).copied(), &class_source).to_string();
    Some(FeatureRef {
        name: name.into(),
        class: class.into(),
        class_source,
        sub_short: None,
        sub_source: None,
        level,
        source,
    })
}

/// Parse `name|class|classSource|subclassShort|subclassSource|level|source`.
#[must_use]
pub fn parse_subclass_feature_uid(uid: &str) -> Option<FeatureRef> {
    let p: Vec<&str> = uid.split('|').collect();
    let name = p.first()?.trim();
    let class = p.get(1)?.trim();
    let class_source = or_default(p.get(2).copied(), "PHB").to_string();
    let sub_short = p.get(3)?.trim().to_string();
    let sub_source = or_default(p.get(4).copied(), "PHB").to_string();
    let level = p.get(5)?.trim().parse().ok()?;
    let source = or_default(p.get(6).copied(), &sub_source).to_string();
    Some(FeatureRef {
        name: name.into(),
        class: class.into(),
        class_source,
        sub_short: Some(sub_short),
        sub_source: Some(sub_source),
        level,
        source,
    })
}

/// Index key of a class feature.
#[must_use]
pub fn class_feature_key(
    name: &str,
    class: &str,
    class_source: &str,
    level: u32,
    source: &str,
) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        name.to_lowercase(),
        class.to_lowercase(),
        class_source.to_uppercase(),
        level,
        source.to_uppercase()
    )
}

/// Index key of a subclass feature.
#[must_use]
pub fn subclass_feature_key(r: &FeatureRef) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        r.name.to_lowercase(),
        r.class.to_lowercase(),
        r.class_source.to_uppercase(),
        r.sub_short.as_deref().unwrap_or("").to_lowercase(),
        r.sub_source.as_deref().unwrap_or("").to_uppercase(),
        r.level,
        r.source.to_uppercase()
    )
}

/// Key of a feature entry that was loaded from data.
#[must_use]
pub fn key_of_feature(f: &Feature, sub: bool) -> String {
    if sub {
        subclass_feature_key(&FeatureRef {
            name: f.name.clone(),
            class: f.class_name.clone(),
            class_source: f.class_source.clone(),
            sub_short: f.subclass_short_name.clone(),
            sub_source: f.subclass_source.clone(),
            level: f.level,
            source: f.source.clone(),
        })
    } else {
        class_feature_key(&f.name, &f.class_name, &f.class_source, f.level, &f.source)
    }
}

/// Allocate the id of a feature reference.
pub fn feature_id(st: &mut State, r: &FeatureRef, sub: bool) -> String {
    let level = format!("L{}", r.level);
    if sub {
        let short = r.sub_short.clone().unwrap_or_default();
        st.ids.id(
            IdKind::SubclassFeature,
            &r.source,
            &[&r.class, &short, &level, &r.name],
            &subclass_feature_key(r),
        )
    } else {
        st.ids.id(
            IdKind::ClassFeature,
            &r.source,
            &[&r.class, &level, &r.name],
            &class_feature_key(&r.name, &r.class, &r.class_source, r.level, &r.source),
        )
    }
}

fn ref_of(f: &Feature, sub: bool) -> FeatureRef {
    FeatureRef {
        name: f.name.clone(),
        class: f.class_name.clone(),
        class_source: f.class_source.clone(),
        sub_short: sub.then(|| f.subclass_short_name.clone().unwrap_or_default()),
        sub_source: sub.then(|| f.subclass_source.clone().unwrap_or_default()),
        level: f.level,
        source: f.source.clone(),
    }
}

/// Convert a class feature or subclass feature.
pub fn convert_feature(sh: &Shared, st: &mut State, ent: &Ent, sub: bool) -> Conv {
    let kind = if sub {
        "subclassFeature"
    } else {
        "classFeature"
    };
    let f: Feature = match model::parse(&ent.raw) {
        Ok(f) => f,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                kind,
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let fref = ref_of(&f, sub);
    st.done_features.insert(format!(
        "{}:{}",
        if sub { "s" } else { "c" },
        key_of_feature(&f, sub)
    ));
    let id = feature_id(st, &fref, sub);
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let ty = if sub {
        "Archetype Feature"
    } else {
        "Class Feature"
    };
    let mut el = new_element(
        &id,
        &f.name,
        ty,
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let mut mech = Mech::new(sh, &f.name);

    if !sub && f.name == "Ability Score Improvement" {
        let class_token = supports_token(&f.class_name);
        let mut rule = if ent.ruleset == borealis_content::Ruleset::Dnd2024 {
            Rule::Select(select(
                "Feat",
                &format!("Feat ({} {})", f.class_name, f.level),
                Some("General"),
                1,
            ))
        } else {
            Rule::Select(select(
                "Class Feature",
                &format!("Improvement Option ({} {})", f.class_name, f.level),
                Some(&format!("Improvement Option&&{class_token}&&{}", f.level)),
                1,
            ))
        };
        set_level(&mut rule, f.level);
        mech.push(rule);
    }
    el.rules = std::mem::take(&mut mech.rules);
    for (k, vs) in &mech.unmapped {
        note_unmapped(&mut el, k, &vs.join("; "));
    }
    let mut description = renderer.entries(&f.entries);
    append_fluff(sh, kind, ent, &mut description, &mut renderer);
    el.description = description;
    el.sheet = Some(sheet_text(&short_text(&Renderer::plain(&f.entries), 300)));
    el.data.insert("level".into(), Value::from(f.level));
    drain_renderer(st, &renderer, &ent.file, &f.name);
    Conv {
        elements: vec![el],
        partial: mech.partial,
    }
}

/// Subclass feature references of a subclass, in order.
fn subclass_feature_refs(sc: &Subclass) -> Vec<FeatureRef> {
    sc.subclass_features
        .iter()
        .filter_map(|v| {
            let uid = match v {
                Value::String(s) => s.as_str(),
                other => other.get("subclassFeature").and_then(Value::as_str)?,
            };
            parse_subclass_feature_uid(uid)
        })
        .collect()
}

/// Text of every feature of a subclass, nested ones included (school restrictions live in
/// prose only; 5etools nests "Spellcasting" under the introductory feature).
fn feature_text(sh: &Shared, sc: &Subclass) -> String {
    let field = |e: &Value, k: &str| e.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    sh.subclass_features
        .iter()
        .filter(|f| {
            field(&f.raw, "className") == sc.class_name
                && field(&f.raw, "subclassShortName") == sc.short_name
                && field(&f.raw, "subclassSource") == sc.source
        })
        .filter_map(|f| f.raw.get("entries"))
        .map(|e| crate::text::strip_tags(&e.to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert a subclass (an `Archetype`).
pub fn convert_subclass(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let sc: Subclass = match model::parse(&ent.raw) {
        Ok(s) => s,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "subclass",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let is_2024 = ent.ruleset == borealis_content::Ruleset::Dnd2024;
    let class_key = format!(
        "{}|{}",
        sc.class_name.to_lowercase(),
        sc.class_source.to_uppercase()
    );
    let class_id = sh.class_ids.get(&class_key).cloned().unwrap_or_else(|| {
        format!(
            "ID_5ET_{}_CLASS_{}",
            crate::util::id_fragment(&sc.class_source),
            crate::util::id_fragment(&sc.class_name)
        )
    });
    let id = st.ids.id(
        IdKind::Subclass,
        &ent.source,
        &[&sc.class_name, &sc.short_name],
        &format!(
            "{class_key}|{}|{}",
            sc.short_name.to_lowercase(),
            ent.source.to_uppercase()
        ),
    );
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let mut el = new_element(
        &id,
        &sc.name,
        "Archetype",
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    el.supports.push(class_id);
    if let Some(title) = sh.subclass_titles.get(&class_key) {
        let tag = supports_token(title);
        if !tag.is_empty() {
            el.supports.push(tag);
        }
    }
    let mut mech = Mech::new(sh, &sc.name);
    let mut partial: Vec<String> = Vec::new();

    // Features: grant each at its level; the introductory feature (named like the subclass)
    // becomes the description.
    let refs = subclass_feature_refs(&sc);
    let min_level = refs.iter().map(|r| r.level).min().unwrap_or(0);
    let mut intro_entries: Vec<Value> = Vec::new();
    for r in &refs {
        let key = subclass_feature_key(r);
        let Some(&idx) = sh.subclass_feature_keys.get(&key) else {
            partial.push(format!(
                "subclass feature {:?} ({}) not found in the data",
                r.name, r.source
            ));
            continue;
        };
        let fe = &sh.subclass_features[idx];
        let is_intro = r.level == min_level
            && (r.name.eq_ignore_ascii_case(&sc.name)
                || r.name.eq_ignore_ascii_case(&sc.short_name));
        if is_intro {
            let body = fe
                .raw
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            intro_entries.extend(body.into_iter().filter(|e| {
                !matches!(
                    e.get("type").and_then(Value::as_str),
                    Some("refSubclassFeature")
                )
            }));
            if st.done_features.insert(format!("s:{key}")) {
                st.report.converted(&fe.file, "subclassFeature");
            }
            continue;
        }
        let fid = feature_id(st, r, true);
        mech.push(grant_at(&fid, "Archetype Feature", r.level));
    }

    // Domain / expanded / innate spells. Spells of a casting class (or of a casting subclass)
    // join its block; a non-casting class's subclass gets a block named after the subclass.
    let caster_sub = sc.caster_progression.is_some();
    let class_casts = caster_sub || sh.is_caster_class(&sc.class_name);
    let label = if class_casts && !caster_sub {
        sc.class_name.clone()
    } else {
        sc.name.clone()
    };
    let own_block = !class_casts;
    let spells = spells_extra::convert(sh, &label, &sc.additional_spells, own_block);
    for r in &spells.direct {
        mech.push(r.clone());
    }
    partial.extend(spells.partial);
    let mut spellcasting: Option<Spellcasting> = None;
    let mut extra_elements = Vec::new();
    if !spells.options.is_empty() {
        let tag = format!("SPELL_OPTION_OF_{}", id.trim_start_matches("ID_"));
        for opt in spells.options {
            let mut oe = new_element(
                &format!("{id}_{}", crate::util::id_fragment(&opt.name)),
                &opt.name,
                "Archetype Feature",
                &el.source,
                ent.ruleset,
            );
            oe.supports = vec![tag.clone()];
            oe.description = format!(
                "<p>Spell option of {}.</p>",
                crate::text::tags::esc(&sc.name)
            );
            oe.sheet = Some(borealis_content::Sheet {
                display: Some(false),
                ..borealis_content::Sheet::default()
            });
            oe.data
                .insert("class".into(), Value::String(sc.class_name.clone()));
            let helpers = spell_blocks::fill(&mut oe, opt.rules, &opt.casting, own_block);
            extra_elements.push(oe);
            extra_elements.extend(helpers);
        }
        mech.push(Rule::Select(super::build::select(
            "Archetype Feature",
            &format!("{} Spells", sc.name),
            Some(&tag),
            1,
        )));
    }

    // Subclass casters (Eldritch Knight and similar).
    let groups: &[Value] = &sc.subclass_table_groups;
    let schools = sc
        .caster_progression
        .is_some()
        .then(|| school_rule(&feature_text(sh, &sc)))
        .flatten();
    let c = caster(&CasterInput {
        owner: &sc.name,
        class_name: &sc.class_name,
        list_tag: &sc.name,
        ability: sc.spellcasting_ability.as_deref(),
        progression: sc.caster_progression.as_deref(),
        groups,
        cantrips: &sc.cantrip_progression,
        known: &sc.spells_known_progression,
        known_fixed: &sc.spells_known_progression_fixed,
        prepared_formula: sc.prepared_spells.as_deref(),
        prepared_progression: &sc.prepared_spells_progression,
        book_allow_lower: sc
            .spells_known_progression_fixed_allow_lower_level
            .unwrap_or(false),
        by_level: sc.spells_known_progression_fixed_by_level.as_ref(),
        prepared_change: sc.prepared_spells_change.as_deref(),
        schools: schools.as_ref(),
        is_2024,
    });
    for r in c.rules {
        mech.push(r);
    }
    partial.extend(c.partial);
    if c.spellcasting.is_some() {
        spellcasting = c.spellcasting;
    }
    let placed = spell_blocks::place(
        &spell_blocks::Carrier {
            id: &id,
            name: &sc.name,
            source: &el.source,
            ruleset: ent.ruleset,
            free: spellcasting.is_none(),
        },
        &spells.casting,
        &spells.direct,
        own_block,
    );
    for r in placed.rules {
        mech.push(r);
    }
    if placed.spellcasting.is_some() {
        spellcasting = placed.spellcasting;
    }
    extra_elements.extend(placed.elements);
    let table = class_table(&sc.name, groups);
    if sc.caster_progression.is_none() {
        for r in table.rules {
            mech.push(r);
        }
    }
    if let Some(t) = table.data {
        el.data.insert("table".into(), t);
    }
    for r in optional_feature_selects(&sc.optionalfeature_progression, &mut partial) {
        mech.push(r);
    }
    for r in feat_selects(&sc.feat_progression, &mut partial) {
        mech.push(r);
    }

    el.spellcasting = spellcasting;
    el.rules = std::mem::take(&mut mech.rules);
    for (k, vs) in &mech.unmapped {
        note_unmapped(&mut el, k, &vs.join("; "));
    }
    partial.extend(std::mem::take(&mut mech.partial));
    let mut description = if intro_entries.is_empty() {
        format!(
            "<p>{} subclass of the {}.</p>",
            crate::text::tags::esc(&sc.name),
            crate::text::tags::esc(&sc.class_name)
        )
    } else {
        renderer.entries(&intro_entries)
    };
    append_fluff(sh, "subclass", ent, &mut description, &mut renderer);
    el.description = description;
    el.sheet = Some(borealis_content::Sheet {
        display: Some(false),
        ..borealis_content::Sheet::default()
    });
    el.data
        .insert("class".into(), Value::String(sc.class_name.clone()));
    if min_level > 0 {
        el.data.insert("level".into(), Value::from(min_level));
    }
    el.maps_to = sh.maps_to(
        SrdKind::Subclass,
        ent.ruleset,
        &sc.name,
        &sc.reprinted_as,
        &|_| None,
    );
    drain_renderer(st, &renderer, &ent.file, &sc.name);
    partial.sort();
    partial.dedup();
    let mut elements = vec![el];
    elements.extend(extra_elements);
    Conv { elements, partial }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Ruleset;
    use serde_json::json;

    fn ent(raw: Value, file: &str) -> Ent {
        Ent {
            name: raw["name"].as_str().unwrap().to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: true,
            file: file.into(),
            raw,
        }
    }

    #[test]
    fn uid_parsing_and_keys() {
        let c = parse_class_feature_uid("Second Wind|Fighter||1").unwrap();
        assert_eq!(
            (c.class_source.as_str(), c.source.as_str(), c.level),
            ("PHB", "PHB", 1)
        );
        let c = parse_class_feature_uid("Martial Versatility|Fighter||4|TCE").unwrap();
        assert_eq!(c.source, "TCE");
        let c = parse_class_feature_uid("Second Wind|Fighter|XPHB|1").unwrap();
        assert_eq!(
            (c.class_source.as_str(), c.source.as_str()),
            ("XPHB", "XPHB")
        );
        assert!(parse_class_feature_uid("broken").is_none());
        let s = parse_subclass_feature_uid("Improved Critical|Fighter||Champion||3").unwrap();
        assert_eq!(
            (
                s.sub_short.as_deref(),
                s.sub_source.as_deref(),
                s.level,
                s.source.as_str()
            ),
            (Some("Champion"), Some("PHB"), 3, "PHB")
        );
        let s = parse_subclass_feature_uid("Heroic Warrior|Fighter|XPHB|Champion|XPHB|10").unwrap();
        assert_eq!(s.source, "XPHB");
        let s2 = parse_subclass_feature_uid("X|Fighter||Champion||3|TCE").unwrap();
        assert_eq!(s2.source, "TCE");
        assert!(parse_subclass_feature_uid("a|b||c").is_none());
        assert_eq!(
            class_feature_key("Rage", "Barbarian", "phb", 1, "phb"),
            "rage|barbarian|PHB|1|PHB"
        );
        let f: Feature = model::parse(&json!({"name": "Rage", "source": "PHB", "className": "Barbarian", "classSource": "PHB", "level": 1})).unwrap();
        assert_eq!(key_of_feature(&f, false), "rage|barbarian|PHB|1|PHB");
    }

    #[test]
    fn class_feature_conversion_and_asi_special_cases() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(
            json!({"name": "Second Wind", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ", "level": 1,
            "entries": ["Regain {@dice 1d10} hit points."]}),
            "class/class-fighter.json",
        );
        let conv = convert_feature(&sh, &mut st, &e, false);
        let el = &conv.elements[0];
        assert_eq!(el.id, "ID_5ET_ZZZ_CLASSFEATURE_FIGHTER_L1_SECOND_WIND");
        assert_eq!(el.ty, "Class Feature");
        assert!(el.description.contains("1d10"));
        assert!(el.rules.is_empty());
        assert_eq!(
            el.sheet.as_ref().unwrap().descriptions[0].text,
            "Regain 1d10 hit points."
        );

        let asi = ent(
            json!({"name": "Ability Score Improvement", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ", "level": 4, "entries": ["x"]}),
            "f.json",
        );
        let el = &convert_feature(&sh, &mut st, &asi, false).elements[0];
        let Rule::Select(s) = &el.rules[0] else {
            panic!()
        };
        assert_eq!(
            (s.ty.as_str(), s.level, s.supports.as_deref()),
            (
                "Class Feature",
                Some(4),
                Some("Improvement Option&&Fighter&&4")
            )
        );
        assert_eq!(s.name, "Improvement Option (Fighter 4)");
        let asi24 = ent(
            json!({"name": "Ability Score Improvement", "source": "XPHB", "edition": "one", "className": "Fighter", "classSource": "XPHB", "level": 8, "entries": ["x"]}),
            "f.json",
        );
        let el = &convert_feature(&sh, &mut st, &asi24, false).elements[0];
        let Rule::Select(s) = &el.rules[0] else {
            panic!()
        };
        assert_eq!(
            (s.ty.as_str(), s.supports.as_deref(), s.name.as_str()),
            ("Feat", Some("General"), "Feat (Fighter 8)")
        );
        assert_eq!(el.ruleset, Some(Ruleset::Dnd2024));
        assert!(st
            .done_features
            .contains("c:ability score improvement|fighter|ZZZ|4|ZZZ"));
    }

    fn shared_with_champion() -> Shared {
        let mut sh = Shared::default();
        sh.class_ids
            .insert("fighter|ZZZ".into(), "ID_5ET_ZZZ_CLASS_FIGHTER".into());
        sh.subclass_titles
            .insert("fighter|ZZZ".into(), "Martial Archetype".into());
        let intro = json!({"name": "Champion", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ", "subclassShortName": "Champion", "subclassSource": "ZZZ", "level": 3,
            "entries": ["Raw physical power.", {"type": "refSubclassFeature", "subclassFeature": "Improved Critical|Fighter|ZZZ|Champion|ZZZ|3"}]});
        let crit = json!({"name": "Improved Critical", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ", "subclassShortName": "Champion", "subclassSource": "ZZZ", "level": 3, "entries": ["19 or 20."]});
        let athlete = json!({"name": "Remarkable Athlete", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ", "subclassShortName": "Champion", "subclassSource": "ZZZ", "level": 7, "entries": ["half"]});
        for raw in [intro, crit, athlete] {
            let e = ent(raw.clone(), "class/class-fighter.json");
            let f: Feature = model::parse(&raw).unwrap();
            sh.subclass_feature_keys
                .insert(key_of_feature(&f, true), sh.subclass_features.len());
            sh.subclass_features.push(e);
        }
        sh
    }

    #[test]
    fn champion_like_subclass() {
        let sh = shared_with_champion();
        let mut st = State::default();
        let e = ent(
            json!({"name": "Champion", "shortName": "Champion", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ",
            "subclassFeatures": ["Champion|Fighter|ZZZ|Champion|ZZZ|3", "Improved Critical|Fighter|ZZZ|Champion|ZZZ|3", "Remarkable Athlete|Fighter|ZZZ|Champion|ZZZ|7", "Missing|Fighter|ZZZ|Champion|ZZZ|10"],
            "optionalfeatureProgression": [{"name": "Fighting Style", "featureType": ["FS:F"], "progression": {"10": 1}}]}),
            "class/class-fighter.json",
        );
        let conv = convert_subclass(&sh, &mut st, &e);
        let el = &conv.elements[0];
        assert_eq!(el.id, "ID_5ET_ZZZ_SUBCLASS_FIGHTER_CHAMPION");
        assert_eq!(el.ty, "Archetype");
        assert_eq!(
            el.supports,
            vec!["ID_5ET_ZZZ_CLASS_FIGHTER", "Martial Archetype"]
        );
        assert!(el.description.contains("Raw physical power."));
        assert!(!el.description.contains("class=\"ref\""));
        let grants: Vec<(String, Option<u32>)> = el
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) => Some((g.id.clone(), g.level)),
                _ => None,
            })
            .collect();
        assert_eq!(
            grants,
            vec![
                (
                    "ID_5ET_ZZZ_SUBCLASSFEATURE_FIGHTER_CHAMPION_L3_IMPROVED_CRITICAL".to_string(),
                    Some(3)
                ),
                (
                    "ID_5ET_ZZZ_SUBCLASSFEATURE_FIGHTER_CHAMPION_L7_REMARKABLE_ATHLETE".to_string(),
                    Some(7)
                ),
            ]
        );
        assert!(el.rules.iter().any(
            |r| matches!(r, Rule::Select(s) if s.name == "Fighting Style" && s.level == Some(10))
        ));
        assert_eq!(conv.partial.len(), 1);
        assert!(conv.partial[0].contains("Missing"));
        assert_eq!(el.data["level"], 3);
        // the intro feature was merged and is not converted again
        assert!(st
            .done_features
            .contains("s:champion|fighter|ZZZ|champion|ZZZ|3|ZZZ"));
    }

    #[test]
    fn domain_spells_and_subclass_casters() {
        let mut sh = Shared::default();
        sh.class_ids
            .insert("cleric|ZZZ".into(), "ID_5ET_ZZZ_CLASS_CLERIC".into());
        let mut st = State::default();
        let e = ent(
            json!({"name": "Life Domain", "shortName": "Life", "source": "ZZZ", "className": "Cleric", "classSource": "ZZZ",
            "subclassFeatures": [],
            "additionalSpells": [{"prepared": {"1": ["bless", "cure wounds"], "3": ["lesser restoration"]}, "expanded": {"1": ["magic missile"]}}]}),
            "f.json",
        );
        let el = &convert_subclass(&sh, &mut st, &e).elements[0];
        let grants: Vec<_> = el
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) => Some((
                    g.id.as_str(),
                    g.level,
                    g.prepared,
                    g.spellcasting.as_deref(),
                )),
                _ => None,
            })
            .collect();
        assert!(grants.contains(&("ID_5ET_PHB_SPELL_BLESS", None, Some(true), Some("Cleric"))));
        assert!(grants.contains(&(
            "ID_5ET_PHB_SPELL_LESSER_RESTORATION",
            Some(3),
            Some(true),
            Some("Cleric")
        )));
        let sc = el.spellcasting.as_ref().unwrap();
        assert_eq!(sc.name.as_deref(), Some("Cleric"));
        assert_eq!(sc.extend, Some(true));
        assert_eq!(sc.extends[0].supports, "ID_5ET_PHB_SPELL_MAGIC_MISSILE");

        let ek = ent(
            json!({"name": "Eldritch Knight", "shortName": "Eldritch Knight", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ",
            "subclassFeatures": [], "casterProgression": "1/3", "spellcastingAbility": "int", "cantripProgression": [0, 0, 2, 2],
            "spellsKnownProgression": [0, 0, 3, 4],
            "subclassTableGroups": [{"title": "Spell Slots per Spell Level", "colLabels": ["1st"], "rowsSpellProgression": [[0,0,0,0,0,0,0,0,0],[0,0,0,0,0,0,0,0,0],[2,0,0,0,0,0,0,0,0],[3,0,0,0,0,0,0,0,0]]}]}),
            "f.json",
        );
        let el = &convert_subclass(&sh, &mut st, &ek).elements[0];
        let sc = el.spellcasting.as_ref().unwrap();
        assert_eq!(sc.name.as_deref(), Some("Eldritch Knight"));
        assert_eq!(sc.list.as_ref().unwrap().supports, "Eldritch Knight");
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Stat(s) if s.name == "eldritch knight:spellcasting:slots:1" && s.level == Some(3) && s.value == "2")));
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id.ends_with("SLOTS_THIRD"))));
        assert!(el.data.contains_key("table"));
        let bad = ent(
            json!({"name": "Bad", "source": "ZZZ", "subclassFeatures": "x"}),
            "f.json",
        );
        assert!(convert_subclass(&sh, &mut st, &bad).elements.is_empty());
    }

    #[test]
    fn subclass_caster_school_restriction_from_feature_text() {
        let mut sh = Shared::default();
        sh.subclass_features.push(ent(
            json!({"name": "Hedge Magic", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ",
            "subclassShortName": "Hedge Knight", "subclassSource": "ZZZ", "level": 3,
            "entries": ["You learn three spells, two of which you must choose from the conjuration and divination spells of that list.",
                "Spells you learn at 9th and 15th level can come from any school."]}),
            "f.json",
        ));
        let mut st = State::default();
        let known: Vec<i64> = vec![0, 0, 3, 3, 3, 3, 3, 3, 4];
        let sub = ent(
            json!({"name": "Hedge Knight", "shortName": "Hedge Knight", "source": "ZZZ", "className": "Fighter", "classSource": "ZZZ",
            "subclassFeatures": [], "casterProgression": "1/3", "spellcastingAbility": "int",
            "cantripProgression": [0, 0, 2], "spellsKnownProgression": known,
            "subclassTableGroups": [{"title": "Spell Slots per Spell Level", "colLabels": ["1st"], "rowsSpellProgression": [[0,0,0,0,0,0,0,0,0],[0,0,0,0,0,0,0,0,0],[2,0,0,0,0,0,0,0,0]]}]}),
            "f.json",
        );
        let el = &convert_subclass(&sh, &mut st, &sub).elements[0];
        let picks: Vec<(&str, &str, u32)> = el
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) if s.ty == "Spell" => Some((
                    s.name.as_str(),
                    s.supports.as_deref().unwrap_or(""),
                    s.number.unwrap_or(1),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            picks,
            [
                ("Cantrip (Hedge Knight 3)", "$(spellcasting:list), 0", 2),
                (
                    "Spell (Hedge Knight 3)",
                    "$(spellcasting:list), $(spellcasting:slots), (Conjuration||Divination)",
                    2
                ),
                (
                    "Spell of Any School (Hedge Knight 3)",
                    "$(spellcasting:list), $(spellcasting:slots)",
                    1
                ),
                (
                    "Spell of Any School (Hedge Knight 9)",
                    "$(spellcasting:list), $(spellcasting:slots)",
                    1
                ),
            ]
        );
        let sc = el.spellcasting.as_ref().unwrap();
        assert_eq!(sc.allow_replace, Some(true));
    }
}
