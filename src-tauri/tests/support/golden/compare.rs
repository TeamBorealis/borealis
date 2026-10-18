//! Expected sheet (`expected/<id>.json`) vs the built character's snapshot, as flat facts.
//!
//! Both sides become `key -> value` maps with the same keys: numbers stay numbers, labels go
//! through [`norm`] (the one normaliser) only where they are part of a key or a text value.

use super::norm;
use serde_json::Value;
use std::collections::BTreeMap;

const ABILITIES: [(&str, &str); 6] = [
    ("Strength", "str"),
    ("Dexterity", "dex"),
    ("Constitution", "con"),
    ("Intelligence", "int"),
    ("Wisdom", "wis"),
    ("Charisma", "cha"),
];
const SPEEDS: [&str; 5] = ["walk", "climb", "swim", "fly", "burrow"];

/// One compared value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fv {
    Num(i64),
    Text(String),
}

impl std::fmt::Display for Fv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fv::Num(n) => write!(f, "{n}"),
            Fv::Text(t) => write!(f, "'{t}'"),
        }
    }
}

pub type Facts = BTreeMap<String, Fv>;

/// One field that differs: `actual` is `None` when Borealis shows no such value.
#[derive(Debug, Clone)]
pub struct Mismatch {
    pub key: String,
    pub expected: Fv,
    pub actual: Option<Fv>,
}

fn num(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v["value"].as_i64())
}

fn put_num(out: &mut Facts, key: String, v: &Value) {
    if let Some(n) = num(v) {
        out.insert(key, Fv::Num(n));
    }
}

fn text(s: &str) -> Fv {
    Fv::Text(s.to_string())
}

fn bool_text(b: bool) -> Fv {
    text(if b { "yes" } else { "no" })
}

/// Parse an attack bonus string (`+7`, `-1`, `7`).
fn bonus(s: &str) -> Option<i64> {
    s.trim().trim_start_matches('+').parse().ok()
}

/// Facts of an expected sheet.
/// `m` is the golden's mapping entry: `sources` maps a spec casting-source name to the name
/// Borealis shows for it, `resources` an expected resource name to the engine's.
pub fn expected(exp: &Value, m: &Value) -> Facts {
    let sources = &m["sources"];
    let mut out = Facts::new();
    for (name, _) in ABILITIES {
        let a = &exp["abilities"][name];
        put_num(&mut out, format!("ability {name} score"), &a["score"]);
        put_num(&mut out, format!("ability {name} modifier"), &a["modifier"]);
        let s = &exp["saving_throws"][name];
        put_num(&mut out, format!("save {name}"), &s["value"]);
        if let Some(p) = s["proficient"].as_bool() {
            out.insert(format!("save {name} proficient"), bool_text(p));
        }
    }
    for (k, field) in [
        ("proficiency bonus", "proficiency_bonus"),
        ("hp max", "hit_points_max"),
        ("armor class", "armor_class"),
        ("initiative", "initiative"),
        ("passive perception", "passive_perception"),
    ] {
        put_num(&mut out, k.to_string(), &exp[field]);
    }
    for k in SPEEDS {
        let v = num(&exp["speeds"][k]).unwrap_or(0);
        out.insert(format!("speed {k}"), Fv::Num(v));
    }
    if let Some(skills) = exp["skills"].as_object() {
        for (name, s) in skills {
            put_num(
                &mut out,
                format!("skill {}", norm::label(name)),
                &s["value"],
            );
            if let Some(p) = s["proficiency"].as_str() {
                out.insert(format!("skill {} proficiency", norm::label(name)), text(p));
            }
        }
    }
    for sc in exp["spellcasting"].as_array().into_iter().flatten() {
        let src = source(sc["source"].as_str().unwrap_or_default(), sources);
        put_num(
            &mut out,
            format!("spellcasting [{src}] dc"),
            &sc["spell_save_dc"],
        );
        put_num(
            &mut out,
            format!("spellcasting [{src}] attack"),
            &sc["spell_attack_bonus"],
        );
    }
    for lv in 1..=9 {
        let n = exp["spell_slots"]["value"][lv.to_string()]
            .as_i64()
            .unwrap_or(0);
        out.insert(format!("slots level {lv}"), Fv::Num(n));
    }
    let pact = &exp["pact_slots"];
    out.insert(
        "pact slots".into(),
        Fv::Num(pact["count"].as_i64().unwrap_or(0)),
    );
    out.insert(
        "pact slot level".into(),
        Fv::Num(pact["slot_level"].as_i64().unwrap_or(0)),
    );
    for a in exp["attacks"].as_array().into_iter().flatten() {
        let name = norm::label(a["name"].as_str().unwrap_or_default());
        if let Some(dc) = a["save"]["dc"].as_i64() {
            out.insert(format!("attack [{name}] save dc"), Fv::Num(dc));
        } else {
            put_num(&mut out, format!("attack [{name}] to hit"), &a["to_hit"]);
        }
        for (k, field) in [
            ("damage", "damage"),
            ("damage two handed", "damage_two_handed"),
        ] {
            if let Some(d) = a[field]["value"].as_str() {
                out.insert(format!("attack [{name}] {k}"), text(&norm::damage(d)));
            }
        }
    }
    for r in exp["resources"].as_array().into_iter().flatten() {
        let raw = r["name"].as_str().unwrap_or_default();
        let raw = m["resources"][raw].as_str().unwrap_or(raw);
        put_num(
            &mut out,
            format!("resource [{}] max", norm::resource(raw)),
            &r["max"],
        );
    }
    if let Some(spells) = exp["spells"].as_object() {
        for (src, s) in spells {
            let src = source(src, sources);
            for (k, field) in [
                ("cantrips max", "cantrips_known_max"),
                ("prepared max", "prepared_max"),
                ("prepared max", "spells_known_max"),
                ("spellbook size", "spellbook_size"),
            ] {
                put_num(&mut out, format!("spells [{src}] {k}"), &s[field]);
            }
            if let Some(list) = s["always_prepared"].as_array() {
                let mut names: Vec<String> = list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(norm::label)
                    .collect();
                names.sort();
                out.insert(
                    format!("spells [{src}] always prepared"),
                    text(&names.join(", ")),
                );
            }
        }
    }
    let mut features: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    collect_expected_features(&exp["features"], &mut features);
    for (name, levels) in features {
        let levels = compared_levels(&name, levels);
        out.insert(format!("feature [{name}] levels"), text(&join(&levels)));
    }
    out
}

fn source(name: &str, aliases: &Value) -> String {
    norm::label(aliases[name].as_str().unwrap_or(name))
}

fn collect_expected_features(v: &Value, out: &mut BTreeMap<String, Vec<i64>>) {
    match v {
        Value::Array(items) => {
            for f in items {
                if let (Some(n), Some(l)) = (f["name"].as_str(), f["level"].as_i64()) {
                    out.entry(norm::label(n)).or_default().push(l);
                }
            }
        }
        Value::Object(map) => map.values().for_each(|x| collect_expected_features(x, out)),
        _ => {}
    }
}

/// Levels compared for a feature: every level for Ability Score Improvement (the expected sheet
/// lists each one), otherwise the lowest level that grants it.
fn compared_levels(name: &str, mut levels: Vec<i64>) -> Vec<i64> {
    levels.sort_unstable();
    levels.dedup();
    if name != ASI {
        levels.truncate(1);
    }
    levels
}

const ASI: &str = "ability score improvement";

fn join(levels: &[i64]) -> String {
    levels
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Facts of a Borealis snapshot (serialised `CharacterSnapshot`), keyed like [`expected`].
/// Feature levels are only read for the names the expected sheet lists.
pub fn actual(snap: &Value, expected: &Facts) -> Facts {
    let mut out = Facts::new();
    let ab = &snap["abilities"];
    for (name, key) in ABILITIES {
        put_num(&mut out, format!("ability {name} score"), &ab["final"][key]);
        put_num(
            &mut out,
            format!("ability {name} modifier"),
            &ab["modifier"][key],
        );
        put_num(&mut out, format!("save {name}"), &ab["save"][key]["bonus"]);
        if let Some(p) = ab["save"][key]["proficient"].as_bool() {
            out.insert(format!("save {name} proficient"), bool_text(p));
        }
    }
    let st = &snap["stats"];
    for (k, field) in [
        ("proficiency bonus", "proficiencyBonus"),
        ("hp max", "hpMax"),
        ("armor class", "armorClass"),
        ("initiative", "initiative"),
        ("passive perception", "passivePerception"),
    ] {
        put_num(&mut out, k.to_string(), &st[field]);
    }
    for k in SPEEDS {
        let v = snap["play"]["speeds"][k].as_i64().unwrap_or(0);
        out.insert(format!("speed {k}"), Fv::Num(v));
    }
    for s in st["skills"].as_array().into_iter().flatten() {
        let name = norm::label(s["name"].as_str().unwrap_or_default());
        put_num(&mut out, format!("skill {name}"), &s["bonus"]);
        if let Some(p) = s["proficiency"].as_str() {
            out.insert(format!("skill {name} proficiency"), text(p));
        }
    }
    spellcasting(snap, &mut out);
    attacks(snap, &mut out);
    for r in snap["play"]["resources"].as_array().into_iter().flatten() {
        let name = norm::resource(r["name"].as_str().unwrap_or_default());
        put_num(&mut out, format!("resource [{name}] max"), &r["max"]);
    }
    for c in snap["play"]["freeCasts"].as_array().into_iter().flatten() {
        let name = norm::resource(c["name"].as_str().unwrap_or_default());
        // `uses` is null when the engine cannot resolve the count (a table it does not read)
        if let Some(n) = c["uses"].as_i64() {
            out.entry(format!("resource [{name}] max"))
                .or_insert(Fv::Num(n));
        }
    }
    features(snap, expected, &mut out);
    out
}

fn spellcasting(snap: &Value, out: &mut Facts) {
    let sc = &snap["spellcasting"];
    let mut slots = [0i64; 9];
    let rows = match sc["multiclassSlots"].as_array() {
        Some(m) if !m.is_empty() => m,
        _ => sc["slots"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default(),
    };
    for r in rows {
        let lv = r["level"].as_u64().unwrap_or(0) as usize;
        if (1..=9).contains(&lv) {
            slots[lv - 1] = r["total"].as_i64().unwrap_or(0);
        }
    }
    for (i, n) in slots.iter().enumerate() {
        out.insert(format!("slots level {}", i + 1), Fv::Num(*n));
    }
    let (mut pact, mut pact_level) = (0, 0);
    for c in sc["classes"].as_array().into_iter().flatten() {
        let src = norm::label(c["className"].as_str().unwrap_or_default());
        put_num(out, format!("spellcasting [{src}] dc"), &c["saveDc"]);
        put_num(
            out,
            format!("spellcasting [{src}] attack"),
            &c["attackBonus"],
        );
        put_num(
            out,
            format!("spells [{src}] cantrips max"),
            &c["cantripsMax"],
        );
        let class_slot = c["classSlotId"].as_str().unwrap_or_default();
        let selects = |pred: &dyn Fn(&str) -> bool| -> i64 {
            let n = snap["slots"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|s| {
                    s["elementType"] == "spell" && s["classSlotId"].as_str() == Some(class_slot)
                })
                .filter(|s| pred(s["title"].as_str().unwrap_or_default()))
                .count();
            i64::try_from(n).unwrap_or(0)
        };
        match c["model"].as_str() {
            // a known caster's limit is the number of spell selects the builder gives it
            Some("known") => {
                let n = selects(&|t| !t.contains("antrip"));
                out.insert(format!("spells [{src}] prepared max"), Fv::Num(n));
            }
            _ => put_num(
                out,
                format!("spells [{src}] prepared max"),
                &c["preparedMax"],
            ),
        }
        if c["model"].as_str() == Some("spellbook") {
            let n = selects(&|t| !t.contains("antrip"));
            out.insert(format!("spells [{src}] spellbook size"), Fv::Num(n));
        }
        let mut always: Vec<String> = c["spells"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| {
                s["alwaysPrepared"].as_bool() == Some(true) && s["level"].as_u64() != Some(0)
            })
            .filter_map(|s| s["name"].as_str())
            .map(norm::label)
            .collect();
        always.sort();
        always.dedup();
        out.insert(
            format!("spells [{src}] always prepared"),
            text(&always.join(", ")),
        );
        if let Some(p) = c["pact"].as_object() {
            pact += p.get("total").and_then(Value::as_i64).unwrap_or(0);
            pact_level = p.get("level").and_then(Value::as_i64).unwrap_or(0);
        }
    }
    out.insert("pact slots".into(), Fv::Num(pact));
    out.insert("pact slot level".into(), Fv::Num(pact_level));
}

fn attacks(snap: &Value, out: &mut Facts) {
    for a in snap["attacks"].as_array().into_iter().flatten() {
        let name = norm::label(a["name"].as_str().unwrap_or_default());
        let bonus_text = a["attackBonus"].as_str().unwrap_or_default();
        let key = if bonus_text.to_ascii_uppercase().contains("DC") {
            let dc = bonus_text
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>();
            dc.parse()
                .ok()
                .map(|n| (format!("attack [{name}] save dc"), n))
        } else {
            bonus(bonus_text).map(|n| (format!("attack [{name}] to hit"), n))
        };
        if let Some((k, n)) = key {
            out.entry(k).or_insert(Fv::Num(n));
        }
        if let Some(d) = a["damage"].as_str() {
            out.entry(format!("attack [{name}] damage"))
                .or_insert_with(|| text(&norm::damage(d)));
        }
    }
}

fn features(snap: &Value, expected: &Facts, out: &mut Facts) {
    let mut levels: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for f in snap["play"]["features"].as_array().into_iter().flatten() {
        let raw = f["name"].as_str().unwrap_or_default();
        let name = norm::feature(raw, f["ty"].as_str().unwrap_or_default());
        levels
            .entry(name)
            .or_default()
            .push(f["level"].as_i64().unwrap_or(0));
    }
    // a sense (2014 Darkvision) is a trait the race grants from level 1
    for s in snap["play"]["senses"].as_array().into_iter().flatten() {
        let name = norm::label(s["name"].as_str().unwrap_or_default());
        levels.entry(name).or_default().push(1);
    }
    for (name, lv) in levels {
        let key = format!("feature [{name}] levels");
        if !expected.contains_key(&key) {
            continue;
        }
        let lv = compared_levels(&name, lv);
        out.insert(key, text(&join(&lv)));
    }
}

/// Every expected fact that Borealis does not show with the same value.
pub fn diff(expected: &Facts, actual: &Facts) -> Vec<Mismatch> {
    expected
        .iter()
        .filter(|(k, v)| actual.get(*k) != Some(*v))
        .map(|(k, v)| Mismatch {
            key: k.clone(),
            expected: v.clone(),
            actual: actual.get(k).cloned(),
        })
        .collect()
}
