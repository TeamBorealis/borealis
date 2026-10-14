//! Tiered free-cast spells (the shape of the bundled Warlock's Mystic Arcanum): at each printed
//! class level a Spell `select` from the class list at that spell level, plus the
//! `mystic_arcanum` rule with the slots and the once-per-rest cast. The model often writes the
//! `mystic_arcanum` slots alone, which offers no pick; the selects are added here.

use serde_json::{json, Value};

use super::types::Edition;

const ORDINALS: [&str; 4] = ["st", "nd", "rd", "th"];

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn num(v: &Value, key: &str) -> Option<u64> {
    v.get(key)
        .and_then(|n| n.as_u64().or_else(|| n.as_str()?.trim().parse().ok()))
}

/// `(spell level, class level)` of every slot of the element's `mystic_arcanum` rules.
fn slots(rules: &[Value]) -> Vec<(u64, u64)> {
    rules
        .iter()
        .filter(|r| str_at(r, "kind") == "mystic_arcanum")
        .flat_map(|r| {
            r.get("slots")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|s| Some((num(&s, "spell_level")?, num(&s, "level")?)))
        .collect()
}

/// The number before an ordinal suffix ending at `end` in `words` ("6th" -> 6).
fn ordinal(word: &str) -> Option<u64> {
    let w = word.trim_matches(|c: char| !c.is_alphanumeric());
    let digits: String = w.chars().take_while(char::is_ascii_digit).collect();
    let rest = &w[digits.len()..];
    (!digits.is_empty() && ORDINALS.contains(&rest))
        .then(|| digits.parse().ok())
        .flatten()
}

/// Slots read from the printed text: "Choose one 6th-level spell from the X spell list ...
/// one 7th-level spell at 13th level, one 8th-level spell at 15th level" (the first at the
/// feature's own level). Only for text that casts them without a slot ("once without").
fn slots_from_text(text: &str, own_level: u64) -> Vec<(u64, u64)> {
    let lower = text.to_lowercase().replace('-', " ");
    if !(lower.contains("spell list") && lower.contains("once without")) {
        return Vec::new();
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut out: Vec<(u64, u64)> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let Some(spell_level) = ordinal(w) else {
            continue;
        };
        if words.get(i + 1).copied() != Some("level")
            || !words
                .get(i + 2)
                .is_some_and(|s| s.trim_matches(|c: char| !c.is_alphanumeric()) == "spell")
        {
            continue;
        }
        let at = (words.get(i + 3).copied() == Some("at"))
            .then(|| words.get(i + 4).and_then(|x| ordinal(x)))
            .flatten();
        let level = match at {
            Some(l) => l,
            None if out.is_empty() => own_level,
            None => continue,
        };
        if !(1..=9).contains(&spell_level) || out.iter().any(|(s, _)| *s == spell_level) {
            continue;
        }
        out.push((spell_level, level));
    }
    if out.len() < 2 {
        return Vec::new();
    }
    out
}

/// The spell list a "from the X spell list" names, title-cased, if any.
fn named_list(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let end = lower.find(" spell list")?;
    let start = lower[..end].rfind("the ")? + 4;
    let name = text.get(start..end)?.trim();
    (!name.is_empty() && name.split_whitespace().count() <= 3).then(|| {
        name.split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' => {
                tag = false;
                out.push(' ');
            }
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Add the missing per-slot Spell selects (and, for a feature whose text prints the tiers but
/// has no rule at all, the `mystic_arcanum` rule). Returns a note when anything was added.
pub(crate) fn with_selects(el: &mut Value, edition: Edition) -> Option<String> {
    let class = el
        .pointer("/data/class")
        .and_then(Value::as_str)
        .filter(|c| !c.trim().is_empty() && !c.starts_with("ID_"))?
        .to_string();
    let name = str_at(el, "name").to_string();
    let text = plain(str_at(el, "description"));
    let own_level = el
        .pointer("/data/level")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let rules: Vec<Value> = el
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut found = slots(&rules);
    let mut add_rule = false;
    if found.is_empty() {
        let spell_selects = rules
            .iter()
            .any(|r| str_at(r, "kind") == "select" && str_at(r, "type") == "Spell");
        if spell_selects {
            return None;
        }
        found = slots_from_text(&text, own_level);
        add_rule = !found.is_empty();
    }
    let list = named_list(&text)
        .filter(|l| !l.eq_ignore_ascii_case(&class))
        .unwrap_or_else(|| class.clone());
    let missing: Vec<(u64, u64)> = found
        .iter()
        .copied()
        .filter(|(_, level)| {
            !rules.iter().any(|r| {
                str_at(r, "kind") == "select"
                    && str_at(r, "type") == "Spell"
                    && num(r, "level").unwrap_or(1) == *level
            })
        })
        .collect();
    if missing.is_empty() {
        return None;
    }
    let sep = if edition == Edition::Y2024 { ", " } else { "," };
    let mut added: Vec<Value> = missing
        .iter()
        .map(|(spell_level, level)| {
            let mut sel = json!({"kind": "select", "type": "Spell",
                "name": format!("{name} (level {spell_level})"), "level": level,
                "supports": format!("{list}{sep}{spell_level}"), "spellcasting": class});
            if edition == Edition::Y2024 {
                sel["prepared"] = json!(true);
            }
            sel
        })
        .collect();
    if add_rule {
        let slots: Vec<Value> = found
            .iter()
            .map(|(s, l)| json!({"spell_level": s, "level": l}))
            .collect();
        added.push(json!({"kind": "mystic_arcanum", "slots": slots,
            "free_cast": {"uses": 1, "recharge": "long_rest"}}));
    }
    let levels: Vec<String> = missing.iter().map(|(s, l)| format!("{s} at {l}")).collect();
    let out = el
        .as_object_mut()?
        .entry("rules")
        .or_insert_with(|| json!([]));
    out.as_array_mut()?.extend(added);
    Some(format!(
        "one {list} spell choice per tier added (spell level at class level: {})",
        levels.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_without_selects_get_one_spell_select_each() {
        let mut el = json!({"name": "Veiled Verse", "data": {"class": "Tidecaller", "level": 11},
            "rules": [{"kind": "mystic_arcanum", "free_cast": {"uses": 1, "recharge": "long_rest"},
                "slots": [{"spell_level": 6, "level": 11}, {"spell_level": 7, "level": 13}]}]});
        let note = with_selects(&mut el, Edition::Y2014).expect("added");
        assert!(note.contains("6 at 11"), "{note}");
        let rules = el["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(
            rules[1],
            json!({"kind": "select", "type": "Spell", "name": "Veiled Verse (level 6)",
                "level": 11, "supports": "Tidecaller,6", "spellcasting": "Tidecaller"})
        );
        assert_eq!(rules[2]["supports"], "Tidecaller,7");
        // Idempotent.
        assert!(with_selects(&mut el, Edition::Y2014).is_none());
        let mut y24 = json!({"name": "V", "data": {"class": "Tidecaller"}, "rules": [
            {"kind": "mystic_arcanum", "slots": [{"spell_level": 9, "level": 17}]}]});
        with_selects(&mut y24, Edition::Y2024).expect("added");
        assert_eq!(y24["rules"][1]["supports"], "Tidecaller, 9");
        assert_eq!(y24["rules"][1]["prepared"], true);
    }

    #[test]
    fn printed_tiers_without_any_rule_become_the_full_shape() {
        let mut el = json!({"name": "Veiled Verse", "data": {"class": "Tidecaller", "level": 11},
            "description": "<p>At 11th level, choose one 6th-level spell from the tidecaller spell list. \
You can cast it once without spending a slot.</p><p>At higher levels you choose more: one 7th-level \
spell at 13th level, one 8th-level spell at 15th level, and one 9th-level spell at 17th level.</p>"});
        with_selects(&mut el, Edition::Y2014).expect("added");
        let rules = el["rules"].as_array().unwrap();
        let levels: Vec<u64> = rules
            .iter()
            .filter(|r| r["kind"] == "select")
            .map(|r| r["level"].as_u64().unwrap())
            .collect();
        assert_eq!(levels, vec![11, 13, 15, 17]);
        assert_eq!(rules[4]["kind"], "mystic_arcanum");
        assert_eq!(rules[4]["slots"][3], json!({"spell_level": 9, "level": 17}));
        // A feature that only mentions spell levels gets nothing.
        let mut other = json!({"name": "X", "data": {"class": "Tidecaller", "level": 3},
            "description": "<p>You learn one 1st-level spell from the tidecaller spell list.</p>"});
        assert!(with_selects(&mut other, Edition::Y2014).is_none());
    }
}
