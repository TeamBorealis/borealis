//! Creature-only `_copy` helpers: the 5etools `_mod` modes that act on a stat block
//! (`addSkills`, `addSenses`, `addSpells`, `replaceSpells`, `removeSpells`) and the creature
//! templates of `bestiary/template.json` (`_copy._templates`).
//!
//! Deterministic, and narrow on purpose: a mode or template this module cannot apply
//! exactly is reported, and the bestiary import then skips that creature instead of keeping
//! a stat block that is wrong.

use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// Proficiency bonus of a challenge rating as 5etools writes it (`"1/4"`, `"17"`,
/// `{"cr": "17", ...}`).
fn proficiency_of(cr: Option<&Value>) -> Option<i64> {
    let text = match cr? {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("cr")?.as_str()?.to_string(),
        _ => return None,
    };
    if text.contains('/') || text == "0" {
        return Some(2);
    }
    let n: i64 = text.parse().ok()?;
    Some(2 + (n - 1).max(0) / 4)
}

/// The creature has no challenge rating (decision IM12): `cr` absent, null, or an object
/// without one (`{"lair": ...}` alone).
fn without_cr(target: &Value) -> bool {
    match target.get("cr") {
        None | Some(Value::Null) => true,
        Some(Value::Object(o)) => o.get("cr").map_or(true, Value::is_null),
        Some(_) => false,
    }
}

/// The ability a skill uses.
fn skill_ability(skill: &str) -> Option<&'static str> {
    Some(match skill {
        "athletics" => "str",
        "acrobatics" | "sleight of hand" | "stealth" => "dex",
        "arcana" | "history" | "investigation" | "nature" | "religion" => "int",
        "animal handling" | "insight" | "medicine" | "perception" | "survival" => "wis",
        "deception" | "intimidation" | "performance" | "persuasion" => "cha",
        _ => return None,
    })
}

fn signed(n: i64) -> String {
    if n < 0 {
        n.to_string()
    } else {
        format!("+{n}")
    }
}

fn bonus_of(v: &Value) -> Option<i64> {
    v.as_str()?.trim().trim_start_matches('+').parse().ok()
}

/// `addSkills`: `{"skills": {"perception": 1}}` gives the skill at the multiple of the
/// proficiency bonus (1 proficient, 2 expertise) plus the ability modifier, unless the
/// creature already has a higher bonus. A creature without a challenge rating (IM12) has no
/// proficiency bonus of its own (a summon uses its summoner's): its skills stay as printed
/// rather than the copy failing.
fn add_skills(target: &mut Value, m: &Value) -> Result<(), String> {
    let skills = m
        .get("skills")
        .and_then(Value::as_object)
        .ok_or("addSkills without skills")?;
    if without_cr(target) {
        return Ok(());
    }
    let pb = proficiency_of(target.get("cr")).ok_or("addSkills: no challenge rating")?;
    let mut added = Vec::new();
    for (skill, mult) in skills {
        let ability = skill_ability(skill).ok_or_else(|| format!("addSkills: {skill}"))?;
        let score = target
            .get(ability)
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("addSkills: no {ability} score"))?;
        let mult = mult.as_i64().ok_or("addSkills: bad multiple")?;
        added.push((skill.clone(), (score - 10).div_euclid(2) + pb * mult));
    }
    let obj = target.as_object_mut().ok_or("addSkills: not an object")?;
    let slot = obj
        .entry("skill")
        .or_insert_with(|| Value::Object(Map::new()));
    let map = slot
        .as_object_mut()
        .ok_or("addSkills: skill is not a map")?;
    for (skill, total) in added {
        let higher = map
            .get(&skill)
            .and_then(bonus_of)
            .is_some_and(|b| b >= total);
        if !higher {
            map.insert(skill, Value::String(signed(total)));
        }
    }
    Ok(())
}

/// `addSenses`: `{"senses": [{"type": "darkvision", "range": 60}]}` adds a sense or widens
/// the range of one the creature has.
fn add_senses(target: &mut Value, m: &Value) -> Result<(), String> {
    let list: Vec<&Value> = match m.get("senses") {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(one @ Value::Object(_)) => vec![one],
        _ => return Err("addSenses without senses".into()),
    };
    let obj = target.as_object_mut().ok_or("addSenses: not an object")?;
    let slot = obj
        .entry("senses")
        .or_insert_with(|| Value::Array(Vec::new()));
    let senses = slot
        .as_array_mut()
        .ok_or("addSenses: senses is not a list")?;
    for s in list {
        let ty = s
            .get("type")
            .and_then(Value::as_str)
            .ok_or("addSenses: sense without type")?
            .to_lowercase();
        let range = s
            .get("range")
            .and_then(Value::as_i64)
            .ok_or("addSenses: sense without range")?;
        let text = format!("{ty} {range} ft.");
        let at = senses.iter().position(|v| {
            v.as_str()
                .is_some_and(|t| crate::text::strip_tags(t).to_lowercase().starts_with(&ty))
        });
        match at {
            Some(i) => {
                let old = senses[i]
                    .as_str()
                    .map(crate::text::strip_tags)
                    .unwrap_or_default();
                let old_range: i64 = old
                    .split_whitespace()
                    .nth(1)
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                if old_range < range {
                    senses[i] = Value::String(text);
                }
            }
            None => senses.push(Value::String(text)),
        }
    }
    Ok(())
}

/// The spell lists a spell mode names: `(key in the spellcasting entry, nested level or
/// frequency key, the mode's value for it)`. `will` has no nested key.
fn spell_targets(m: &Value) -> Vec<(String, Option<String>, Value)> {
    let mut out = Vec::new();
    let Some(obj) = m.as_object() else {
        return out;
    };
    for (key, v) in obj {
        if key == "mode" {
            continue;
        }
        match v {
            Value::Object(levels) => {
                for (level, inner) in levels {
                    out.push((key.clone(), Some(level.clone()), inner.clone()));
                }
            }
            other => out.push((key.clone(), None, other.clone())),
        }
    }
    out
}

/// The spell array of a spellcasting entry at `key` / `level` (created when `create`).
fn spell_list<'a>(
    casting: &'a mut Value,
    key: &str,
    level: Option<&str>,
    create: bool,
) -> Option<&'a mut Vec<Value>> {
    let obj = casting.as_object_mut()?;
    if !create && !obj.contains_key(key) {
        return None;
    }
    let slot = obj.entry(key.to_string()).or_insert_with(|| match level {
        Some(_) => Value::Object(Map::new()),
        None => Value::Array(Vec::new()),
    });
    let Some(level) = level else {
        return slot.as_array_mut();
    };
    let levels = slot.as_object_mut()?;
    if !create && !levels.contains_key(level) {
        return None;
    }
    let inner = levels.entry(level.to_string()).or_insert_with(|| {
        if key == "spells" {
            serde_json::json!({ "spells": [] })
        } else {
            Value::Array(Vec::new())
        }
    });
    if key == "spells" {
        let o = inner.as_object_mut()?;
        o.entry("spells")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
    } else {
        inner.as_array_mut()
    }
}

/// `addSpells` / `replaceSpells` / `removeSpells` on the creature's first spellcasting entry
/// (as 5etools applies them).
fn edit_spells(target: &mut Value, mode: &str, m: &Value) -> Result<(), String> {
    let casting = target
        .get_mut("spellcasting")
        .and_then(Value::as_array_mut)
        .and_then(|a| a.first_mut())
        .ok_or_else(|| format!("{mode}: the creature has no spellcasting"))?;
    for (key, level, value) in spell_targets(m) {
        let create = mode == "addSpells";
        let list = spell_list(casting, &key, level.as_deref(), create)
            .ok_or_else(|| format!("{mode}: no {key} {}", level.as_deref().unwrap_or("")))?;
        let items: Vec<Value> = match value {
            Value::Object(o) if key == "spells" => o
                .get("spells")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            Value::Array(a) => a,
            other => vec![other],
        };
        for item in items {
            match mode {
                "addSpells" => list.push(item),
                "removeSpells" => {
                    let before = list.len();
                    list.retain(|s| *s != item);
                    if list.len() == before {
                        return Err(format!("removeSpells: {item} not found"));
                    }
                }
                _ => {
                    let (Some(from), Some(with)) = (item.get("replace"), item.get("with")) else {
                        return Err("replaceSpells without replace/with".into());
                    };
                    let at = list
                        .iter()
                        .position(|s| s == from)
                        .ok_or_else(|| format!("replaceSpells: {from} not found"))?;
                    list[at] = with.clone();
                }
            }
        }
    }
    Ok(())
}

const SIZES: [&str; 6] = ["T", "S", "M", "L", "H", "G"];

/// `maxSize`: sizes above `max` become `max` (a reduced-threat creature is at most Large).
fn max_size(target: &mut Value, m: &Value) -> Result<(), String> {
    let max = m
        .get("max")
        .and_then(Value::as_str)
        .ok_or("maxSize without max")?;
    let cap = SIZES
        .iter()
        .position(|s| *s == max)
        .ok_or("maxSize: unknown size")?;
    let Some(Value::Array(sizes)) = target.get_mut("size") else {
        return Err("maxSize: no size".into());
    };
    let mut out: Vec<Value> = Vec::new();
    for s in sizes.iter() {
        let i = s.as_str().and_then(|c| SIZES.iter().position(|x| *x == c));
        let v = Value::String(SIZES[i.ok_or("maxSize: unknown size")?.min(cap)].to_string());
        if !out.contains(&v) {
            out.push(v);
        }
    }
    *sizes = out;
    Ok(())
}

/// Standard XP of a challenge rating written as 5etools text.
fn standard_xp(cr: &str) -> Option<f64> {
    borealis_content::Cr::parse(cr).map(|c| f64::from(c.standard_xp()))
}

/// `scalarMultXp`: the creature's XP times `scalar` (floored when `floor`), written as the
/// printed XP of its challenge rating (`{"cr": "5", "xp": 900}`). A creature without a
/// challenge rating (IM12) has no XP to scale: nothing changes rather than the copy failing.
fn scalar_mult_xp(target: &mut Value, m: &Value) -> Result<(), String> {
    let by = m
        .get("scalar")
        .and_then(Value::as_f64)
        .ok_or("scalarMultXp without scalar")?;
    if without_cr(target) {
        return Ok(());
    }
    let floor = m.get("floor").and_then(Value::as_bool).unwrap_or(false);
    let cr = target
        .get("cr")
        .ok_or("scalarMultXp: no challenge rating")?;
    let (text, xp) = match cr {
        Value::String(s) => (s.clone(), standard_xp(s)),
        Value::Object(o) => {
            let s = o
                .get("cr")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let xp = o
                .get("xp")
                .and_then(Value::as_f64)
                .or_else(|| standard_xp(&s));
            (s, xp)
        }
        _ => return Err("scalarMultXp: bad challenge rating".into()),
    };
    let xp = xp.ok_or("scalarMultXp: challenge rating is not a number")? * by;
    let xp = if floor { xp.floor() } else { xp.round() };
    let mut obj = match cr {
        Value::Object(o) => o.clone(),
        _ => Map::new(),
    };
    obj.insert("cr".into(), Value::String(text));
    obj.insert("xp".into(), Value::from(xp as i64));
    target["cr"] = Value::Object(obj);
    Ok(())
}

/// `scalarAddHit` / `scalarAddDc`: every `{@hit N}` / `{@dc N}` in the property's text gets
/// `scalar` added.
fn scalar_add_tag(target: &mut Value, key: &str, tag: &str, m: &Value) -> Result<(), String> {
    let by = m
        .get("scalar")
        .and_then(Value::as_i64)
        .ok_or("scalarAdd without scalar")?;
    let re = regex::Regex::new(&format!(r"\{{@{tag} (-?\d+)\}}")).map_err(|e| e.to_string())?;
    fn walk(v: &mut Value, re: &regex::Regex, tag: &str, by: i64) {
        match v {
            Value::String(s) => {
                let out = re.replace_all(s, |c: &regex::Captures<'_>| {
                    let n: i64 = c[1].parse().unwrap_or(0);
                    format!("{{@{tag} {}}}", n + by)
                });
                *s = out.into_owned();
            }
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, re, tag, by)),
            Value::Object(o) => o.values_mut().for_each(|x| walk(x, re, tag, by)),
            _ => {}
        }
    }
    if let Some(v) = target.get_mut(key) {
        walk(v, &re, tag, by);
    }
    Ok(())
}

/// `prefixSuffixStringProp`: text around a string property (`hp.formula`).
fn prefix_suffix(target: &mut Value, key: &str, m: &Value) -> Result<(), String> {
    let prop = m
        .get("prop")
        .and_then(Value::as_str)
        .ok_or("prefixSuffixStringProp without prop")?;
    let text = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if let Some(Value::String(s)) = target.get_mut(key).and_then(|v| v.get_mut(prop)) {
        *s = format!("{}{s}{}", text("prefix"), text("suffix"));
    }
    Ok(())
}

/// Apply a creature-only `_mod` mode on property `key` (`_` for the whole creature). `None`
/// when `mode` is not one of them.
pub(crate) fn apply_mode(
    target: &mut Value,
    key: &str,
    mode: &str,
    m: &Value,
) -> Option<Result<(), String>> {
    Some(match mode {
        "addSkills" => add_skills(target, m),
        "addSenses" => add_senses(target, m),
        "addSpells" | "replaceSpells" | "removeSpells" => edit_spells(target, mode, m),
        "maxSize" => max_size(target, m),
        "scalarMultXp" => scalar_mult_xp(target, m),
        "scalarAddHit" => scalar_add_tag(target, key, "hit", m),
        "scalarAddDc" => scalar_add_tag(target, key, "dc", m),
        "prefixSuffixStringProp" => prefix_suffix(target, key, m),
        _ => return None,
    })
}

/// Lookup key of a template or of a `_templates` reference.
#[must_use]
pub fn template_key(v: &Value) -> String {
    let f = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase()
    };
    format!("{}|{}", f("name"), f("source"))
}

/// Apply the templates a `_copy` names (`_templates`), in order: each template's `_root`
/// properties replace the copied ones (not those the copying entry sets itself), then its
/// `_mod` runs. A template that is missing is reported in `notes`.
pub(crate) fn apply_templates(
    merged: &mut Value,
    own: &BTreeSet<String>,
    refs: &[Value],
    templates: &std::collections::HashMap<String, Value>,
    notes: &mut Vec<String>,
) {
    for r in refs {
        let Some(t) = templates.get(&template_key(r)) else {
            notes.push(format!("template {} not found", template_key(r)));
            continue;
        };
        let apply = t.get("apply");
        if let (Some(Value::Object(root)), Value::Object(obj)) =
            (apply.and_then(|a| a.get("_root")), &mut *merged)
        {
            for (k, v) in root {
                if !own.contains(k) {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        if let Some(mods) = apply.and_then(|a| a.get("_mod")) {
            crate::copy::apply_mods_pub(merged, mods, notes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn add_skills_uses_the_challenge_rating_and_keeps_a_higher_bonus() {
        let mut m = json!({"cr": "5", "wis": 14, "int": 10, "skill": {"insight": "+9"}});
        let r = apply_mode(
            &mut m,
            "_",
            "addSkills",
            &json!({"skills": {"perception": 1, "investigation": 2, "insight": 1}}),
        );
        assert_eq!(r, Some(Ok(())));
        assert_eq!(m["skill"]["perception"], "+5");
        assert_eq!(m["skill"]["investigation"], "+6");
        assert_eq!(m["skill"]["insight"], "+9");
    }

    #[test]
    fn xp_and_skill_modes_leave_a_creature_without_a_challenge_rating_as_it_is() {
        for cr in [None, Some(Value::Null), Some(json!({"lair": "1"}))] {
            let mut m = json!({"wis": 14, "skill": {"insight": "+2"}});
            if let Some(cr) = cr {
                m["cr"] = cr;
            }
            let before = m.clone();
            let skills = json!({"skills": {"perception": 1}});
            assert_eq!(apply_mode(&mut m, "_", "addSkills", &skills), Some(Ok(())));
            let half = json!({"scalar": 0.5, "floor": true});
            assert_eq!(apply_mode(&mut m, "_", "scalarMultXp", &half), Some(Ok(())));
            assert_eq!(m, before);
        }
        let mut unknown = json!({"cr": "Unknown", "wis": 14});
        let skills = json!({"skills": {"perception": 1}});
        assert!(matches!(
            apply_mode(&mut unknown, "_", "addSkills", &skills),
            Some(Err(_))
        ));
    }

    #[test]
    fn add_senses_widens_or_adds() {
        let mut m = json!({"senses": ["darkvision 30 ft."]});
        let mode = json!({"senses": [{"type": "darkvision", "range": 60}, {"type": "tremorsense", "range": 10}]});
        assert_eq!(apply_mode(&mut m, "_", "addSenses", &mode), Some(Ok(())));
        assert_eq!(
            m["senses"],
            json!(["darkvision 60 ft.", "tremorsense 10 ft."])
        );
    }

    #[test]
    fn spell_modes_edit_the_first_spellcasting_entry() {
        let mut m = json!({"spellcasting": [{
            "will": ["{@spell light}"],
            "daily": {"1e": ["{@spell fear}", "{@spell fly}"]},
            "spells": {"3": {"slots": 3, "spells": ["{@spell fly}"]}}
        }]});
        let add = json!({"will": ["{@spell mage hand}"], "spells": {"5": {"spells": ["{@spell scrying}"]}}});
        assert_eq!(apply_mode(&mut m, "_", "addSpells", &add), Some(Ok(())));
        let rep = json!({"spells": {"3": [{"replace": "{@spell fly}", "with": "{@spell haste}"}]}});
        assert_eq!(apply_mode(&mut m, "_", "replaceSpells", &rep), Some(Ok(())));
        let rem = json!({"daily": {"1e": ["{@spell fear}"]}});
        assert_eq!(apply_mode(&mut m, "_", "removeSpells", &rem), Some(Ok(())));
        let sc = &m["spellcasting"][0];
        assert_eq!(sc["will"], json!(["{@spell light}", "{@spell mage hand}"]));
        assert_eq!(
            sc["spells"]["3"],
            json!({"slots": 3, "spells": ["{@spell haste}"]})
        );
        assert_eq!(sc["spells"]["5"]["spells"], json!(["{@spell scrying}"]));
        assert_eq!(sc["daily"]["1e"], json!(["{@spell fly}"]));
        let missing = json!({"daily": {"1e": [{"replace": "{@spell x}", "with": "{@spell y}"}]}});
        assert!(matches!(
            apply_mode(&mut m, "_", "replaceSpells", &missing),
            Some(Err(_))
        ));
        assert_eq!(apply_mode(&mut m, "_", "calculateProp", &json!({})), None);
    }

    #[test]
    fn reduced_threat_modes_scale_the_stat_block() {
        let mut m = json!({"size": ["H", "G"], "cr": "5",
            "hp": {"average": 11, "formula": "2d8 + 2"},
            "action": [{"name": "Bite", "entries": ["{@hit 6} to hit. {@dc 14} Strength."]}]});
        for (key, mode, spec) in [
            ("_", "maxSize", json!({"max": "L"})),
            ("_", "scalarMultXp", json!({"scalar": 0.5, "floor": true})),
            ("action", "scalarAddHit", json!({"scalar": -2})),
            ("action", "scalarAddDc", json!({"scalar": -2})),
            (
                "hp",
                "prefixSuffixStringProp",
                json!({"prop": "formula", "prefix": "(", "suffix": ") / 2"}),
            ),
        ] {
            assert_eq!(apply_mode(&mut m, key, mode, &spec), Some(Ok(())), "{mode}");
        }
        assert_eq!(m["size"], json!(["L"]));
        assert_eq!(m["cr"], json!({"cr": "5", "xp": 900}));
        assert_eq!(
            m["action"][0]["entries"][0],
            "{@hit 4} to hit. {@dc 12} Strength."
        );
        assert_eq!(m["hp"]["formula"], "(2d8 + 2) / 2");
    }
}
