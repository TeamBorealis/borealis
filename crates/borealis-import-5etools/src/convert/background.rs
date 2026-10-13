//! Backgrounds (2014 style with features and characteristics, 2024 style with ability
//! scores and an origin feat).

use super::build::{grant, select};
use super::mech::{AbilitySource, Mech};
use super::spell_blocks::{self, Carrier};
use super::spells_extra;
use super::{
    append_fluff, drain_renderer, new_element, note_unmapped, sheet_text, short_text, Conv, Ent,
    Shared, State,
};
use crate::ids::IdKind;
use crate::model::{self, Background};
use crate::srd::SrdKind;
use crate::text::{strip_tags, RefKind, Renderer};
use borealis_content::{ListItem, Rule, SelectRule, Setter};
use serde_json::Value;

/// Name of a background characteristic table, from a column label.
fn characteristic_name(label: &str) -> Option<(&'static str, u32)> {
    let l = label.to_lowercase();
    if l.contains("personality") {
        Some(("Personality Trait", 2))
    } else if l.contains("ideal") {
        Some(("Ideal", 1))
    } else if l.contains("bond") {
        Some(("Bond", 1))
    } else if l.contains("flaw") {
        Some(("Flaw", 1))
    } else {
        None
    }
}

fn find_tables<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Array(a) => a.iter().for_each(|x| find_tables(x, out)),
        Value::Object(_) => {
            if value.get("type").and_then(Value::as_str) == Some("table") {
                out.push(value);
            }
            if let Some(e) = value.get("entries") {
                find_tables(e, out);
            }
        }
        _ => {}
    }
}

/// `List` selects for the personality / ideal / bond / flaw tables of a background.
#[must_use]
pub fn characteristic_selects(entries: &[Value]) -> Vec<Rule> {
    let mut tables = Vec::new();
    for e in entries {
        find_tables(e, &mut tables);
    }
    let mut rules = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for t in tables {
        let labels: Vec<String> = t
            .get("colLabels")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(strip_tags).collect())
            .unwrap_or_default();
        let Some((name, number)) = labels.iter().skip(1).find_map(|l| characteristic_name(l))
        else {
            continue;
        };
        if !seen.insert(name) {
            continue;
        }
        let rows = t
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let items: Vec<ListItem> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                let cell = row.as_array()?.last()?;
                let text = match cell {
                    Value::String(s) => strip_tags(s),
                    other => other.to_string(),
                };
                Some(ListItem {
                    id: i64::try_from(i + 1).ok()?,
                    text,
                    ..ListItem::default()
                })
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let mut sel: SelectRule = select("List", name, None, number);
        sel.items = items;
        rules.push(Rule::Select(sel));
    }
    rules
}

fn summary(bg: &Background) -> String {
    let mut parts = Vec::new();
    for obj in &bg.skill_proficiencies {
        if let Some(m) = obj.as_object() {
            for (k, v) in m {
                if v == &Value::Bool(true) {
                    parts.push(crate::util::title_case(k));
                } else if k == "any" {
                    parts.push(format!("{v} skills"));
                } else if k == "choose" {
                    parts.push("a choice of skills".to_string());
                }
            }
        }
    }
    for obj in bg
        .tool_proficiencies
        .iter()
        .chain(&bg.skill_tool_language_proficiencies)
    {
        if let Some(m) = obj.as_object() {
            for (k, v) in m {
                if v == &Value::Bool(true) {
                    parts.push(crate::util::title_case(k));
                }
            }
        }
    }
    let mut langs = 0u64;
    for obj in &bg.language_proficiencies {
        if let Some(m) = obj.as_object() {
            for (k, v) in m {
                if k.starts_with("any") || k == "other" {
                    langs += v.as_u64().unwrap_or(1);
                }
            }
        }
    }
    if langs > 0 {
        parts.push(format!(
            "{langs} Language{}",
            if langs == 1 { "" } else { "s" }
        ));
    }
    parts.join(", ")
}

/// Convert one background.
pub fn convert(sh: &Shared, st: &mut State, ent: &Ent) -> Conv {
    let bg: Background = match model::parse(&ent.raw) {
        Ok(b) => b,
        Err(e) => {
            st.report.skipped(
                &ent.file,
                "background",
                &ent.label(),
                &format!("invalid shape: {e}"),
            );
            return Conv::default();
        }
    };
    let resolver = |k: RefKind, n: &str, s: &str| sh.link(k, n, s);
    let mut renderer = Renderer::new(&resolver);
    let source_display = sh.source_display(&ent.source);
    let id = st.ids.named(IdKind::Background, &ent.source, &bg.name);
    let mut el = new_element(&id, &bg.name, "Background", &source_display, ent.ruleset);
    let mut mech = Mech::new(sh, &bg.name);
    let mut partial = Vec::new();

    mech.ability(&bg.ability, AbilitySource::Background);
    mech.feats(&bg.feats);
    mech.skills(&bg.skill_proficiencies);
    mech.tools(&bg.tool_proficiencies);
    mech.languages(&bg.language_proficiencies);
    mech.weapons(&bg.weapon_proficiencies);
    mech.armor(&bg.armor_proficiencies);
    mech.skill_tool_language(&bg.skill_tool_language_proficiencies);
    let spells = spells_extra::convert_inheriting(
        sh,
        &bg.name,
        &bg.additional_spells,
        true,
        &spells_extra::inherited_abilities(&bg.ability),
    );
    let placed = spell_blocks::place(
        &Carrier {
            id: &id,
            name: &bg.name,
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
    let spell_helpers = placed.elements;
    partial.extend(spells.partial);
    if !spells.options.is_empty() {
        partial
            .push("alternative additional spell groups of a background were not converted".into());
    }

    // "Feature: X" sections become Background Feature elements.
    let mut kept: Vec<Value> = Vec::new();
    let mut features: Vec<borealis_content::Element> = Vec::new();
    for e in &bg.entries {
        let name = e.get("name").and_then(Value::as_str).unwrap_or("");
        let is_feature = e.get("data").and_then(|d| d.get("isFeature")) == Some(&Value::Bool(true))
            || name.starts_with("Feature:");
        if !is_feature {
            kept.push(e.clone());
            continue;
        }
        let fname = name
            .strip_prefix("Feature:")
            .unwrap_or(name)
            .trim()
            .to_string();
        let body = e
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let fid = st.ids.id(
            IdKind::BackgroundFeature,
            &ent.source,
            &[&bg.name, &fname],
            &format!("{}|{}|{}", bg.name, ent.source, fname),
        );
        let mut fe = new_element(
            &fid,
            &fname,
            "Background Feature",
            &source_display,
            ent.ruleset,
        );
        fe.description = renderer.entries(&body);
        fe.sheet = Some(sheet_text(&short_text(&Renderer::plain(&body), 300)));
        mech.push(grant(&fid, "Background Feature"));
        features.push(fe);
    }
    for r in characteristic_selects(&kept) {
        mech.push(r);
    }

    el.setters.push(Setter::new("short", summary(&bg)));
    el.rules = std::mem::take(&mut mech.rules);
    for (k, vs) in &mech.unmapped {
        note_unmapped(&mut el, k, &vs.join("; "));
    }
    partial.extend(std::mem::take(&mut mech.partial));
    if !bg.starting_equipment.is_empty() {
        el.data.insert(
            "starting_equipment".into(),
            Value::Array(bg.starting_equipment.clone()),
        );
    }
    let mut description = renderer.entries(&kept);
    append_fluff(sh, "background", ent, &mut description, &mut renderer);
    el.description = description;
    el.sheet = Some(borealis_content::Sheet {
        display: Some(false),
        ..borealis_content::Sheet::default()
    });
    el.maps_to = sh.maps_to(
        SrdKind::Background,
        ent.ruleset,
        &bg.name,
        &bg.reprinted_as,
        &|uid| sh.reprint(RefKind::Background, uid),
    );
    drain_renderer(st, &renderer, &ent.file, &bg.name);
    partial.sort();
    partial.dedup();
    let mut elements = vec![el];
    elements.extend(features);
    elements.extend(spell_helpers);
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
            file: "backgrounds.json".into(),
            raw,
        }
    }

    #[test]
    fn classic_background_with_feature_and_characteristics() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(json!({
        "name": "Hermit", "source": "ZZZ",
        "skillProficiencies": [{"medicine": true, "religion": true}],
        "languageProficiencies": [{"anyStandard": 1}],
        "toolProficiencies": [{"herbalism kit": true}],
        "startingEquipment": [{"_": ["scroll case|phb"]}],
        "entries": [
            {"type": "list", "items": [{"type": "item", "name": "Skills:", "entry": "Medicine, Religion"}]},
            {"type": "entries", "name": "Feature: Discovery", "entries": ["You found something."], "data": {"isFeature": true}},
            {"type": "entries", "name": "Suggested Characteristics", "entries": [
                "Pick.",
                {"type": "table", "colLabels": ["d4", "Personality Trait"], "rows": [[1, "Quiet."], [2, "Chatty."]]},
                {"type": "table", "colLabels": ["d4", "Ideal"], "rows": [[1, "Free. (Chaotic)"]]},
                {"type": "table", "colLabels": ["d4", "Bond"], "rows": [[1, "A hut."]]},
                {"type": "table", "colLabels": ["d4", "Flaw"], "rows": [[1, "Paranoid."]]},
                {"type": "table", "colLabels": ["d4", "Mystery"], "rows": [[1, "?"]]}
            ]}
        ]}));
        let conv = convert(&sh, &mut st, &e);
        assert!(conv.partial.is_empty());
        let bg = &conv.elements[0];
        assert_eq!(bg.id, "ID_5ET_ZZZ_BACKGROUND_HERMIT");
        assert_eq!(
            bg.setter_value("short"),
            Some("Medicine, Religion, Herbalism Kit, 1 Language")
        );
        let feature = &conv.elements[1];
        assert_eq!(feature.ty, "Background Feature");
        assert_eq!(feature.name, "Discovery");
        assert!(feature.description.contains("You found something."));
        assert!(!bg.description.contains("You found something."));
        assert!(bg
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id == feature.id)));
        assert!(bg
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_PROFICIENCY_SKILL_MEDICINE")));
        assert!(bg.rules.iter().any(|r| matches!(r, Rule::Select(s) if s.ty == "Language" && s.supports.as_deref() == Some("Standard"))));
        let lists: Vec<_> = bg
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) if s.is_list() => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(lists.len(), 4);
        assert_eq!(lists[0].name, "Personality Trait");
        assert_eq!(lists[0].number, Some(2));
        assert_eq!(lists[0].items.len(), 2);
        assert_eq!(lists[0].items[1].id, 2);
        assert_eq!(lists[0].items[1].text, "Chatty.");
        assert!(bg.data.contains_key("starting_equipment"));
    }

    #[test]
    fn origin_background_2024() {
        let mut sh = Shared::default();
        sh.links.insert(
            (RefKind::Feat, "alert".into(), "XPHB".into()),
            "ID_5ET_XPHB_FEAT_ALERT".into(),
        );
        let mut st = State::default();
        let e = ent(json!({
            "name": "Crook", "source": "XPHB", "edition": "one",
            "ability": [{"choose": {"weighted": {"from": ["dex", "con", "int"], "weights": [2, 1]}}},
                        {"choose": {"weighted": {"from": ["dex", "con", "int"], "weights": [1, 1, 1]}}}],
            "feats": [{"alert|xphb": true}],
            "skillProficiencies": [{"sleight of hand": true, "stealth": true}],
            "toolProficiencies": [{"thieves' tools": true}],
            "entries": [{"type": "list", "items": []}]}));
        let bg = &convert(&sh, &mut st, &e).elements[0];
        assert_eq!(bg.ruleset, Some(Ruleset::Dnd2024));
        assert!(bg
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Custom(c) if c.kind == "select.ability_distribution")));
        assert!(bg
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_5ET_XPHB_FEAT_ALERT")));
        assert!(bg.rules.iter().any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS")));
        assert_eq!(
            bg.setter_value("short"),
            Some("Sleight Of Hand, Stealth, Thieves' Tools")
        );
    }

    #[test]
    fn bad_shape_and_named_spell_groups() {
        let sh = Shared::default();
        let mut st = State::default();
        let e = ent(json!({"name": "Mage", "source": "ZZZ",
            "additionalSpells": [{"name": "A", "known": {"1": ["light"]}}], "entries": []}));
        let conv = convert(&sh, &mut st, &e);
        // a single group applies directly, whatever its name
        assert!(conv.partial.is_empty());
        let el = &conv.elements[0];
        assert_eq!(
            el.spellcasting.as_ref().and_then(|b| b.name.as_deref()),
            Some("Mage")
        );
        assert!(el.rules.iter().any(|r| matches!(r, Rule::Grant(g) if g.id == "ID_5ET_PHB_SPELL_LIGHT" && g.spellcasting.as_deref() == Some("Mage"))));
        let bad = ent(json!({"name": "Bad", "source": "ZZZ", "feats": "alert"}));
        assert!(convert(&sh, &mut st, &bad).elements.is_empty());
    }
}
