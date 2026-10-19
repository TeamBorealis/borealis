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
/// creature already has a higher bonus.
fn add_skills(target: &mut Value, m: &Value) -> Result<(), String> {
    let skills = m
        .get("skills")
        .and_then(Value::as_object)
        .ok_or("addSkills without skills")?;
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

/// Apply a creature-only `_mod` mode. `None` when `mode` is not one of them.
pub(crate) fn apply_mode(target: &mut Value, mode: &str, m: &Value) -> Option<Result<(), String>> {
    Some(match mode {
        "addSkills" => add_skills(target, m),
        "addSenses" => add_senses(target, m),
        "addSpells" | "replaceSpells" | "removeSpells" => edit_spells(target, mode, m),
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
            "addSkills",
            &json!({"skills": {"perception": 1, "investigation": 2, "insight": 1}}),
        );
        assert_eq!(r, Some(Ok(())));
        assert_eq!(m["skill"]["perception"], "+5");
        assert_eq!(m["skill"]["investigation"], "+6");
        assert_eq!(m["skill"]["insight"], "+9");
    }

    #[test]
    fn add_senses_widens_or_adds() {
        let mut m = json!({"senses": ["darkvision 30 ft."]});
        let mode = json!({"senses": [{"type": "darkvision", "range": 60}, {"type": "tremorsense", "range": 10}]});
        assert_eq!(apply_mode(&mut m, "addSenses", &mode), Some(Ok(())));
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
        assert_eq!(apply_mode(&mut m, "addSpells", &add), Some(Ok(())));
        let rep = json!({"spells": {"3": [{"replace": "{@spell fly}", "with": "{@spell haste}"}]}});
        assert_eq!(apply_mode(&mut m, "replaceSpells", &rep), Some(Ok(())));
        let rem = json!({"daily": {"1e": ["{@spell fear}"]}});
        assert_eq!(apply_mode(&mut m, "removeSpells", &rem), Some(Ok(())));
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
            apply_mode(&mut m, "replaceSpells", &missing),
            Some(Err(_))
        ));
        assert_eq!(apply_mode(&mut m, "maxSize", &json!({})), None);
    }
}
