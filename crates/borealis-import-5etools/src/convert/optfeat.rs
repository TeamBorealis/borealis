//! Optional features (invocations, metamagic, fighting styles, maneuvers, ...), progression
//! selects shared with classes and feats, and prerequisite conversion.

use super::build::{select, set_level};
use super::mech::feat_category_label;
use super::prereq::apply_prerequisites;
use super::spell_blocks;
use super::spells_extra;
use super::{
    append_fluff, drain_renderer, new_element, sheet_text, short_text, Conv, Ent, Shared, State,
};
use crate::ids::IdKind;
use crate::model::{self, OptionalFeature};
use crate::text::{RefKind, Renderer};
use borealis_content::Rule;
use serde_json::Value;

/// Display label of an optional feature type code.
#[must_use]
pub fn type_label(code: &str) -> String {
    match code {
        "ED" => "Elemental Discipline",
        "EI" => "Eldritch Invocation",
        "MM" => "Metamagic",
        "MV" | "MV:B" => "Maneuver",
        "MV:C2-UA" => "Maneuver (UA)",
        "FS:F" | "FS:B" | "FS:P" | "FS:R" => "Fighting Style",
        "AS" | "AS:V1-UA" | "AS:V2-UA" => "Arcane Shot",
        "PB" => "Pact Boon",
        "AI" => "Artificer Infusion",
        "OR" => "Onomancy Resonant",
        "RN" => "Rune",
        "TT" => "Trick",
        "AF" => "Alchemical Formula",
        "RP" => "Replicate Magic Item",
        "OTH" => "Other",
        other => other,
    }
    .to_string()
}

/// Support tag that identifies one optional feature type (colons are not allowed in tags).
#[must_use]
pub fn type_tag(code: &str) -> String {
    format!("OF_{}", code.replace(':', "-"))
}

fn progression_levels(progression: &Value, partial: &mut Vec<String>) -> Vec<(u32, u32)> {
    match progression {
        Value::Array(a) => {
            let mut prev = 0i64;
            let mut out = Vec::new();
            for (i, v) in a.iter().enumerate() {
                let n = v.as_i64().unwrap_or(prev);
                if n > prev {
                    if let (Ok(delta), Ok(level)) = (u32::try_from(n - prev), u32::try_from(i + 1))
                    {
                        out.push((level, delta));
                    }
                }
                prev = n;
            }
            out
        }
        // `{"3": 2, "7": 3}`: like the array form, the values are running totals ("2 known at
        // level 3, 3 known at level 7"), so each level adds the difference.
        Value::Object(m) => {
            let mut totals = Vec::new();
            for (k, v) in m {
                let Some(n) = v.as_u64().and_then(|n| u32::try_from(n).ok()) else {
                    continue;
                };
                match k.parse::<u32>() {
                    Ok(level) => totals.push((level, n)),
                    // "*": from whenever the owner (a feat) is gained, which a level-1 gate is
                    Err(_) if k == "*" => totals.push((1, n)),
                    Err(_) => {
                        partial.push(format!("progression key {k:?} was applied at level 1"));
                        totals.push((1, n));
                    }
                }
            }
            totals.sort_unstable();
            let mut prev = 0;
            let mut out = Vec::new();
            for (level, n) in totals {
                if n > prev {
                    out.push((level, n - prev));
                    prev = n;
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Selects for `optionalfeatureProgression` entries.
#[must_use]
/// `option_codes`: for a 2024 owner, the codes 2024 optional features carry
/// ([`super::Shared::option_codes_2024`]); a select naming one of them is typed `Option`.
pub fn optional_feature_selects(
    progs: &[Value],
    option_codes: Option<&std::collections::BTreeSet<String>>,
    partial: &mut Vec<String>,
) -> Vec<Rule> {
    let mut rules = Vec::new();
    for p in progs {
        let name = p
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Optional Feature");
        let codes: Vec<String> = p
            .get("featureType")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if codes.is_empty() {
            continue;
        }
        let supports = codes
            .iter()
            .map(|c| type_tag(c))
            .collect::<Vec<_>>()
            .join("||");
        for (level, count) in
            progression_levels(p.get("progression").unwrap_or(&Value::Null), partial)
        {
            let ty = if option_codes.is_some_and(|set| codes.iter().any(|c| set.contains(c))) {
                OPTION_TYPE
            } else {
                "Class Feature"
            };
            let mut rule = Rule::Select(select(ty, name, Some(&supports), count));
            if let Err(e) = set_level(&mut rule, level) {
                partial.push(e);
            }
            rules.push(rule);
        }
    }
    rules
}

/// Selects for `featProgression` entries.
#[must_use]
pub fn feat_selects(progs: &[Value], partial: &mut Vec<String>) -> Vec<Rule> {
    let mut rules = Vec::new();
    for p in progs {
        let name = p.get("name").and_then(Value::as_str).unwrap_or("Feat");
        let cats: Vec<String> = p
            .get("category")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|c| feat_category_label(c).to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if cats.is_empty() {
            continue;
        }
        let supports = cats.join("||");
        for (level, count) in
            progression_levels(p.get("progression").unwrap_or(&Value::Null), partial)
        {
            let mut rule = Rule::Select(select("Feat", name, Some(&supports), count));
            if let Err(e) = set_level(&mut rule, level) {
                partial.push(e);
            }
            rules.push(rule);
        }
    }
    rules
}

/// Element type of 2024 optional features and of the 2024 selects that choose them.
pub const OPTION_TYPE: &str = "Option";

/// Convert one optional feature.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let of: OptionalFeature = match model::parse(&ent.raw) {
        Ok(o) => o,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "optionalfeature",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let id = st.ids.named(IdKind::OptionalFeature, &ent.source, &of.name);
    let ty = if ent.ruleset == borealis_content::Ruleset::Dnd2024 {
        OPTION_TYPE
    } else {
        "Class Feature"
    };
    let mut el = new_element(
        &id,
        &of.name,
        ty,
        &sh.source_display(&ent.source),
        ent.ruleset,
    );
    let mut supports: Vec<String> = Vec::new();
    for code in &of.feature_type {
        let label = type_label(code);
        if !supports.contains(&label) {
            supports.push(label);
        }
        supports.push(type_tag(code));
    }
    el.supports = supports;
    let mut partial = Vec::new();
    apply_prerequisites(sh, st, &of.prerequisite, &mut el, &mut partial);
    let mut description = renderer.entries(&of.entries);
    append_fluff(sh, "optionalfeature", ent, &mut description, &mut renderer);
    el.description = description;
    el.sheet = Some(sheet_text(&short_text(&Renderer::plain(&of.entries), 300)));
    el.data
        .insert("feature_types".into(), serde_json::json!(of.feature_type));
    let (label, own_block) = spell_label(sh, &of.feature_type);
    let spells = spells_extra::convert(sh, ent.ruleset, &label, &of.additional_spells, own_block);
    partial.extend(spells.partial);
    if !spells.options.is_empty() {
        partial.push(
            "alternative additional spell groups of an optional feature were not converted".into(),
        );
    }
    let helpers = spell_blocks::fill(&mut el, spells.direct, &spells.casting, own_block);
    drain_renderer(st, &renderer, &ent.file, &of.name);
    let mut elements = vec![el];
    elements.extend(helpers);
    Conv { elements, partial }
}

/// The class whose spells an optional feature grants (invocations are Warlock spells), or the
/// feature type as a spell source of its own (Elemental Disciplines).
fn spell_label(sh: &Shared, codes: &[String]) -> (String, bool) {
    let class = codes.iter().find_map(|c| match c.as_str() {
        "EI" | "PB" => Some("Warlock"),
        "FS:P" => Some("Paladin"),
        "FS:R" => Some("Ranger"),
        "MM" => Some("Sorcerer"),
        "AI" => Some("Artificer"),
        _ => None,
    });
    match class {
        Some(c) if sh.is_caster_class(c) => (c.to_string(), false),
        _ => (
            codes
                .first()
                .map_or_else(|| "Optional Feature".into(), |c| type_label(c)),
            true,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn labels_and_tags() {
        assert_eq!(type_label("FS:F"), "Fighting Style");
        assert_eq!(type_label("EI"), "Eldritch Invocation");
        assert_eq!(type_label("ZZ"), "ZZ");
        assert_eq!(type_tag("FS:F"), "OF_FS-F");
        assert_eq!(type_tag("MV:C2-UA"), "OF_MV-C2-UA");
    }

    #[test]
    fn selects_of_2024_option_codes_are_typed_option() {
        // F13: the bundled SRD 5.2.1 types invocation / metamagic selects and options `Option`
        let codes: std::collections::BTreeSet<String> = ["EI".to_string()].into();
        let progs = [
            json!({"name": "Eldritch Invocations", "featureType": ["EI"], "progression": {"1": 1}}),
            json!({"name": "Fighting Style", "featureType": ["FS:F"], "progression": {"1": 1}}),
        ];
        let mut partial = Vec::new();
        let tys = |rules: Vec<Rule>| -> Vec<String> {
            rules
                .into_iter()
                .filter_map(|r| match r {
                    Rule::Select(s) => Some(s.ty),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            tys(optional_feature_selects(&progs, Some(&codes), &mut partial)),
            ["Option", "Class Feature"]
        );
        assert_eq!(
            tys(optional_feature_selects(&progs, None, &mut partial)),
            ["Class Feature", "Class Feature"]
        );
        let sh = Shared::default();
        let mut st = State::default();
        let raw = json!({"name": "Agonizing Blast", "source": "XPHB", "edition": "one",
            "featureType": ["EI"], "entries": ["x"]});
        let ent = Ent {
            name: "Agonizing Blast".into(),
            source: "XPHB".into(),
            ruleset: borealis_content::Ruleset::Dnd2024,
            is_srd: false,
            file: "optionalfeatures.json".into(),
            raw,
        };
        let conv = convert(&sh, &mut st, &ent);
        assert_eq!(conv.elements[0].ty, "Option");
        assert_eq!(crate::pack::file_for(&conv.elements[0]), "options.json");
    }

    #[test]
    fn progression_selects_for_arrays_objects_and_wildcards() {
        let mut partial = Vec::new();
        let rules = optional_feature_selects(
            &[
                json!({"name": "Eldritch Invocations", "featureType": ["EI"],
                       "progression": [0, 2, 2, 2, 3, 3, 4, 4, 5, 5, 5, 6, 6, 6, 7, 7, 7, 8, 8, 8]}),
                json!({"name": "Fighting Style", "featureType": ["FS:F", "FS:P"], "progression": {"1": 1, "10": 2}}),
                json!({"name": "Maneuvers", "featureType": ["MV:B"], "progression": {"3": 3, "7": 5, "10": 7, "15": 9}}),
                json!({"name": "Odd", "featureType": ["OTH"], "progression": {"*": 1}}),
                json!({"name": "Odder", "featureType": ["OTH"], "progression": {"x": 1}}),
                json!({"name": "None", "featureType": [], "progression": {"1": 1}}),
            ],
            None,
            &mut partial,
        );
        let sel: Vec<_> = rules
            .iter()
            .map(|r| match r {
                Rule::Select(s) => (
                    s.name.clone(),
                    s.level.unwrap_or(1),
                    s.number.unwrap_or(1),
                    s.supports.clone().unwrap(),
                ),
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            sel[0],
            ("Eldritch Invocations".into(), 2, 2, "OF_EI".into())
        );
        assert_eq!(
            sel[1],
            ("Eldritch Invocations".into(), 5, 1, "OF_EI".into())
        );
        assert_eq!(sel[2].1, 7);
        assert!(sel.contains(&("Fighting Style".into(), 1, 1, "OF_FS-F||OF_FS-P".into())));
        assert!(sel.contains(&("Fighting Style".into(), 10, 1, "OF_FS-F||OF_FS-P".into())));
        assert!(sel.contains(&("Odd".into(), 1, 1, "OF_OTH".into())));
        // object values are running totals: 3 maneuvers at 3, then 2 more at 7, 10 and 15
        let maneuvers: Vec<(u32, u32)> = sel
            .iter()
            .filter(|s| s.0 == "Maneuvers")
            .map(|s| (s.1, s.2))
            .collect();
        assert_eq!(maneuvers, vec![(3, 3), (7, 2), (10, 2), (15, 2)]);
        // "*" (whenever the feat is gained) is exact; only the unknown key is reported
        assert_eq!(
            partial,
            vec!["progression key \"x\" was applied at level 1".to_string()]
        );
        assert!(!sel.iter().any(|s| s.0 == "None"));
    }

    #[test]
    fn feat_progression_selects() {
        let mut partial = Vec::new();
        let rules = feat_selects(
            &[
                json!({"name": "Fighting Style", "category": ["FS"], "progression": {"1": 1}}),
                json!({"name": "Epic Boon", "category": ["EB"], "progression": {"19": 1}}),
            ],
            &mut partial,
        );
        assert_eq!(rules.len(), 2);
        let Rule::Select(s) = &rules[1] else { panic!() };
        assert_eq!(
            (s.ty.as_str(), s.level, s.supports.as_deref()),
            ("Feat", Some(19), Some("Epic Boon Feat"))
        );
    }

    #[test]
    fn converts_an_invocation_with_prerequisites() {
        let sh = Shared::default();
        let mut st = State::default();
        let raw = json!({"name": "Spiky Gaze", "source": "ZZZ", "featureType": ["EI"],
            "prerequisite": [{"level": {"level": 5, "class": {"name": "Warlock"}}, "pact": "Chain"}],
            "entries": ["Your gaze deals {@damage 1d6} damage."]});
        let ent = Ent {
            name: "Spiky Gaze".into(),
            source: "ZZZ".into(),
            ruleset: borealis_content::Ruleset::Dnd2014,
            is_srd: false,
            file: "optionalfeatures.json".into(),
            raw,
        };
        let conv = convert(&sh, &mut st, &ent);
        let el = &conv.elements[0];
        assert_eq!(el.id, "ID_5ET_ZZZ_OPTFEATURE_SPIKY_GAZE");
        assert_eq!(el.ty, "Class Feature");
        assert_eq!(el.supports, vec!["Eldritch Invocation", "OF_EI"]);
        assert_eq!(el.requirements, "[warlock:5]");
        assert!(el.prerequisite.contains("Warlock"));
        assert_eq!(conv.partial.len(), 1);
        assert!(el.description.contains("1d6"));
        assert!(el.extra.attributes.contains_key("5et:prerequisite"));
    }
}
