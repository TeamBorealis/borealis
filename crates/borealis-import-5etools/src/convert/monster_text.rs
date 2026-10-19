//! Printed lines of a 5etools creature that become plain text in the stat block: alignment,
//! a feature name's usage ("(3/Day)", "{@recharge 5}", "(Costs 2 Actions)"), senses and the
//! spellcasting trait's spell lists.

use crate::text::{strip_tags, Renderer};
use borealis_content::monster::{RestKind, Senses, Usage};
use serde_json::Value;

fn one_alignment(code: &str) -> &'static str {
    match code {
        "L" => "lawful",
        "C" => "chaotic",
        "G" => "good",
        "E" => "evil",
        "U" => "unaligned",
        "A" => "any alignment",
        _ => "neutral",
    }
}

/// The alignment of a list of codes (`["C", "E"]`, `["L", "NX", "C", "E"]`).
fn codes_text(codes: &[&str]) -> String {
    let has = |c: &str| codes.contains(&c);
    let all_axis = has("L") && has("NX") && has("C");
    let all_moral = has("G") && has("NY") && has("E");
    match codes.len() {
        0 => "none".into(),
        1 => one_alignment(codes[0]).into(),
        2 if codes == ["N", "N"] => "neutral".into(),
        2 => format!("{} {}", one_alignment(codes[0]), one_alignment(codes[1])),
        4 if all_axis && (has("G") || has("E")) => {
            format!("any {} alignment", if has("G") { "good" } else { "evil" })
        }
        4 if all_moral && (has("L") || has("C")) => {
            format!(
                "any {} alignment",
                if has("L") { "lawful" } else { "chaotic" }
            )
        }
        5 if all_axis && has("NY") => {
            format!(
                "any non-{} alignment",
                if has("G") { "evil" } else { "good" }
            )
        }
        5 if all_moral && has("NX") => {
            format!(
                "any non-{} alignment",
                if has("L") { "chaotic" } else { "lawful" }
            )
        }
        _ if all_axis && all_moral => "any alignment".into(),
        _ => codes
            .iter()
            .map(|c| one_alignment(c))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// The printed alignment (`chaotic evil`, `any non-good alignment`, `neutral good (50%) or
/// neutral evil (50%)`), with the `alignmentPrefix` ("typically "). `none` when the book
/// prints none.
#[must_use]
pub fn alignment(raw: &Value) -> String {
    let Some(list) = raw.get("alignment").and_then(Value::as_array) else {
        return "none".into();
    };
    let codes: Vec<&str> = list.iter().filter_map(Value::as_str).collect();
    let text = if codes.len() == list.len() {
        codes_text(&codes)
    } else {
        list.iter()
            .filter_map(|a| {
                if let Some(s) = a.get("special").and_then(Value::as_str) {
                    return Some(strip_tags(s));
                }
                let inner: Vec<&str> = a
                    .get("alignment")?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                let mut t = codes_text(&inner);
                if let Some(c) = a.get("chance").and_then(Value::as_i64) {
                    t.push_str(&format!(" ({c}%)"));
                }
                if let Some(n) = a.get("note").and_then(Value::as_str) {
                    t.push_str(&format!(" ({})", strip_tags(n)));
                }
                Some(t)
            })
            .collect::<Vec<_>>()
            .join(" or ")
    };
    let prefix = raw
        .get("alignmentPrefix")
        .and_then(Value::as_str)
        .map(strip_tags)
        .unwrap_or_default();
    let text = format!("{prefix}{text}").trim().to_string();
    if text.is_empty() {
        "none".into()
    } else {
        text
    }
}

fn number_before(s: &str, marker: &str) -> Option<u8> {
    let at = s.find(marker)?;
    s[..at]
        .trim_end()
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// A feature name split into its printed name and usage: `Fire Breath {@recharge 5}` ->
/// (`Fire Breath`, recharge 5-6); `Legendary Resistance (3/Day, or 4/Day in Lair)`;
/// `Wing Attack (Costs 2 Actions)` -> cost 2.
#[must_use]
pub fn name_usage(raw_name: &str) -> (String, Option<Usage>, Option<u8>) {
    let mut name = raw_name.to_string();
    let mut usage = None;
    if let Some(at) = name.find("{@recharge") {
        let end = name[at..].find('}').map_or(name.len(), |e| at + e + 1);
        let n: u8 = name[at + "{@recharge".len()..end - 1]
            .trim()
            .parse()
            .unwrap_or(6);
        usage = Some(Usage::Recharge { min: n, max: 6 });
        name.replace_range(at..end, "");
    }
    let mut name = strip_tags(&name).trim().to_string();
    let mut cost = None;
    if let (Some(open), true) = (name.rfind(" ("), name.ends_with(')')) {
        let inner = name[open + 2..name.len() - 1].to_lowercase();
        let parsed = if inner.starts_with("costs ") {
            cost = number_before(&inner, " action");
            cost.is_some()
        } else if let Some(normal) = number_before(&inner, "/day") {
            let lair = inner
                .split_once(" or ")
                .filter(|(_, rest)| rest.contains("in lair"))
                .and_then(|(_, rest)| number_before(rest, "/day"));
            usage = Some(match lair {
                Some(lair) => Usage::PerDayInLair { normal, lair },
                None => Usage::PerDay(normal),
            });
            true
        } else if inner.starts_with("recharges after a short or long rest") {
            usage = Some(Usage::OnRest(RestKind::ShortOrLong));
            true
        } else if inner.starts_with("recharges after a long rest") {
            usage = Some(Usage::OnRest(RestKind::Long));
            true
        } else {
            false
        };
        if parsed {
            name.truncate(open);
        }
    }
    (name, usage, cost)
}

/// Senses as printed (`darkvision 60 ft.`, `blindsight 30 ft. (blind beyond this radius)`)
/// with the passive Perception.
#[must_use]
pub fn senses(raw: &Value, passive: u32) -> Senses {
    let mut out = Senses {
        passive_perception: passive,
        ..Senses::default()
    };
    let list: Vec<String> = match raw.get("senses") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(strip_tags).collect(),
        Some(Value::String(s)) => s.split(", ").map(strip_tags).collect(),
        _ => Vec::new(),
    };
    for s in list {
        let lower = s.to_lowercase();
        let mut words = lower.split_whitespace();
        let kind = words.next().unwrap_or("");
        let range: Option<u32> = words.next().and_then(|n| n.parse().ok());
        let slot = match kind {
            "blindsight" => &mut out.blindsight,
            "darkvision" => &mut out.darkvision,
            "tremorsense" => &mut out.tremorsense,
            "truesight" => &mut out.truesight,
            _ => {
                out.notes.push(s.clone());
                continue;
            }
        };
        match range {
            Some(r) if slot.is_none() => *slot = Some(r),
            _ => {
                out.notes.push(s.clone());
                continue;
            }
        }
        if lower.contains("blind beyond") {
            out.blind_beyond = true;
        }
        let rest = lower.split_once("ft.").map_or("", |(_, r)| r.trim());
        if !rest.is_empty() && !lower.contains("blind beyond") {
            out.notes.push(s.clone());
        }
    }
    out
}

fn spell_names(v: &Value) -> String {
    let list: Vec<String> = match v {
        Value::Array(a) => a
            .iter()
            .filter_map(|s| match s {
                Value::String(t) => Some(strip_tags(t)),
                other => other.get("entry").and_then(Value::as_str).map(strip_tags),
            })
            .collect(),
        Value::Object(o) => o.get("spells").map(spell_names).into_iter().collect(),
        _ => Vec::new(),
    };
    list.join(", ")
}

fn ordinal(level: &str) -> String {
    let suffix = match level {
        "1" => "st",
        "2" => "nd",
        "3" => "rd",
        _ => "th",
    };
    format!("{level}{suffix} level")
}

/// `3/day each`, `1/day`, `2/rest each`, `1/week` for a frequency key (`3e`, `1`, ...).
fn per(key: &str, unit: &str) -> String {
    match key.strip_suffix('e') {
        Some(n) => format!("{n}/{unit} each"),
        None => format!("{key}/{unit}"),
    }
}

fn frequency_lines(sc: &Value, field: &str, unit: &str, lines: &mut Vec<String>) {
    let Some(map) = sc.get(field).and_then(Value::as_object) else {
        return;
    };
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.trim_end_matches('e').parse::<u32>().unwrap_or(0)));
    for k in keys {
        lines.push(format!("{}: {}", per(k, unit), spell_names(&map[k])));
    }
}

/// The text of a spellcasting entry: header, the spell lists by frequency and level, footer.
#[must_use]
pub fn spellcasting(sc: &Value) -> String {
    let entries = |k: &str| {
        sc.get(k)
            .and_then(Value::as_array)
            .map(|a| Renderer::plain(a))
            .unwrap_or_default()
    };
    let mut lines = Vec::new();
    if let Some(will) = sc.get("will") {
        lines.push(format!("At will: {}", spell_names(will)));
    }
    if let Some(map) = sc.get("ritual") {
        lines.push(format!("Rituals: {}", spell_names(map)));
    }
    for (field, unit) in [
        ("daily", "day"),
        ("rest", "rest"),
        ("restLong", "long rest"),
        ("weekly", "week"),
        ("monthly", "month"),
        ("yearly", "year"),
    ] {
        frequency_lines(sc, field, unit, &mut lines);
    }
    if let Some(levels) = sc.get("spells").and_then(Value::as_object) {
        let mut keys: Vec<&String> = levels.keys().collect();
        keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(0));
        for k in keys {
            let l = &levels[k];
            let head = match (k.as_str(), l.get("slots").and_then(Value::as_i64)) {
                ("0", _) => "Cantrips (at will)".to_string(),
                (_, Some(n)) => {
                    format!("{} ({n} slot{})", ordinal(k), if n == 1 { "" } else { "s" })
                }
                _ => ordinal(k),
            };
            lines.push(format!("{head}: {}", spell_names(l)));
        }
    }
    let mut parts = vec![entries("headerEntries")];
    parts.extend(lines);
    parts.push(entries("footerEntries"));
    parts.retain(|p| !p.trim().is_empty());
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn alignments_read_as_printed() {
        let a = |v: Value| alignment(&json!({ "alignment": v }));
        assert_eq!(a(json!(["C", "E"])), "chaotic evil");
        assert_eq!(a(json!(["U"])), "unaligned");
        assert_eq!(a(json!(["N"])), "neutral");
        assert_eq!(a(json!(["A"])), "any alignment");
        assert_eq!(a(json!(["L", "NX", "C", "E"])), "any evil alignment");
        assert_eq!(
            a(json!(["L", "NX", "C", "NY", "E"])),
            "any non-good alignment"
        );
        assert_eq!(
            a(json!(["NX", "C", "G", "NY", "E"])),
            "any non-lawful alignment"
        );
        assert_eq!(a(json!(["C", "G", "NY", "E"])), "any chaotic alignment");
        assert_eq!(
            a(
                json!([{"alignment": ["N", "G"], "chance": 50}, {"alignment": ["N", "E"], "chance": 50}])
            ),
            "neutral good (50%) or neutral evil (50%)"
        );
        assert_eq!(
            a(json!([{"special": "host's alignment"}])),
            "host's alignment"
        );
        assert_eq!(alignment(&json!({})), "none");
        assert_eq!(
            alignment(&json!({"alignment": ["C", "E"], "alignmentPrefix": "typically "})),
            "typically chaotic evil"
        );
    }

    #[test]
    fn feature_names_give_their_usage() {
        assert_eq!(
            name_usage("Fire Breath {@recharge 5}"),
            (
                "Fire Breath".into(),
                Some(Usage::Recharge { min: 5, max: 6 }),
                None
            )
        );
        assert_eq!(
            name_usage("Breath {@recharge}").1,
            Some(Usage::Recharge { min: 6, max: 6 })
        );
        assert_eq!(
            name_usage("Legendary Resistance (3/Day, or 4/Day in Lair)"),
            (
                "Legendary Resistance".into(),
                Some(Usage::PerDayInLair { normal: 3, lair: 4 }),
                None
            )
        );
        assert_eq!(
            name_usage("Legendary Resistance (3/Day)").1,
            Some(Usage::PerDay(3))
        );
        assert_eq!(
            name_usage("Wing Attack (Costs 2 Actions)"),
            ("Wing Attack".into(), None, Some(2))
        );
        assert_eq!(
            name_usage("Shapechanger (Human Form Only)"),
            ("Shapechanger (Human Form Only)".into(), None, None)
        );
    }

    #[test]
    fn senses_are_typed_and_the_rest_kept_as_notes() {
        let s = senses(
            &json!({"senses": ["blindsight 30 ft. (blind beyond this radius)", "Darkvision 120 ft.", "{@sense tremorsense} 60 ft.", "see invisible"]}),
            12,
        );
        assert_eq!(s.blindsight, Some(30));
        assert!(s.blind_beyond);
        assert_eq!(s.darkvision, Some(120));
        assert_eq!(s.tremorsense, Some(60));
        assert_eq!(s.notes, vec!["see invisible".to_string()]);
        assert_eq!(s.passive_perception, 12);
    }

    #[test]
    fn spellcasting_lists_frequencies_and_levels() {
        let text = spellcasting(&json!({
            "headerEntries": ["The mage casts spells ({@dc 14})."],
            "will": ["{@spell light}"],
            "daily": {"1": ["{@spell fly}"], "3e": ["{@spell fear}", "{@spell sleep}"]},
            "spells": {"0": {"spells": ["{@spell mage hand}"]}, "1": {"slots": 4, "spells": ["{@spell shield}"]}},
            "footerEntries": ["Done."]
        }));
        assert_eq!(
            text,
            "The mage casts spells (DC 14).\nAt will: light\n3/day each: fear, sleep\n1/day: fly\n\
             Cantrips (at will): mage hand\n1st level (4 slots): shield\nDone."
        );
    }
}
