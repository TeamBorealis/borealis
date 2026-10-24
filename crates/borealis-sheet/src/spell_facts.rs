//! Spell facts from content: casting time, range, components, duration, concentration, ritual
//! and a short deterministic "Effect" line built from the spell's SRD description.

use crate::model_types::SpellFacts;
use borealis_content::Element;

/// Longest effect text, in characters.
pub const EFFECT_MAX_CHARS: usize = 90;

const HIGHER_LEVEL_MARKERS: [&str; 4] = [
    "At Higher Levels",
    "Using a Higher-Level Spell Slot",
    "Cantrip Upgrade",
    "Using a Higher",
];

const ABILITIES: [(&str, &str); 6] = [
    ("strength", "STR"),
    ("dexterity", "DEX"),
    ("constitution", "CON"),
    ("intelligence", "INT"),
    ("wisdom", "WIS"),
    ("charisma", "CHA"),
];

/// Facts of a spell element (SRD 5.1 or 5.2.1 shape).
#[must_use]
pub fn spell_facts(el: &Element) -> SpellFacts {
    let flag = |name: &str| el.setter_value(name).is_some_and(|v| v.trim() == "true");
    let time = el.setter_value("time").map(str::trim).unwrap_or_default();
    let duration = el
        .setter_value("duration")
        .map(str::trim)
        .unwrap_or_default();
    let concentration = flag("isConcentration") || duration.starts_with("Concentration");
    let ritual = flag("isRitual") || time.contains("Ritual");
    let mut parts = Vec::new();
    for (key, letter) in [
        ("hasVerbalComponent", "V"),
        ("hasSomaticComponent", "S"),
        ("hasMaterialComponent", "M"),
    ] {
        if flag(key) {
            parts.push(letter);
        }
    }
    SpellFacts {
        casting_time: casting_time(time),
        range: non_empty(el.setter_value("range")),
        components: (!parts.is_empty()).then(|| parts.join(", ")),
        duration: duration_text(duration),
        concentration,
        ritual,
        effect: spell_effect(&el.description),
        attack_roll: attack_roll(&el.description),
        save: save_ability(&el.description).map(String::from),
    }
}

/// The description up to the higher-level text, as plain text.
fn base_text(description_html: &str) -> String {
    let full = plain(description_html);
    let cut = HIGHER_LEVEL_MARKERS
        .iter()
        .filter_map(|m| full.find(m))
        .min()
        .unwrap_or(full.len());
    full[..cut].trim().to_string()
}

fn attack_roll(description_html: &str) -> bool {
    base_text(description_html)
        .to_lowercase()
        .contains("spell attack")
}

/// The save ability of a spell that is not resolved with an attack roll.
fn save_ability(description_html: &str) -> Option<&'static str> {
    let text = base_text(description_html);
    if text.to_lowercase().contains("spell attack") {
        return None;
    }
    first_save(&text.split_whitespace().collect::<Vec<_>>())
}

fn non_empty(v: Option<&str>) -> Option<String> {
    v.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// "Action or Ritual" -> "Action"; "1 minute or Ritual" -> "1 minute" (the ritual flag carries it); a trailing
/// ", which you take ..." trigger is dropped.
fn casting_time(time: &str) -> Option<String> {
    let base = time.split(" or Ritual").next().unwrap_or(time).trim();
    let base = base.strip_suffix(" or ritual").unwrap_or(base);
    // "1 reaction, which you take when ...": the trigger is spell text, not part of the casting time.
    let base = base.split(',').next().unwrap_or(base).trim();
    (!base.is_empty()).then(|| base.to_string())
}

/// "Concentration, up to 1 minute" -> "Up to 1 minute" (the concentration flag carries the rest).
fn duration_text(duration: &str) -> Option<String> {
    let rest = duration
        .strip_prefix("Concentration, ")
        .map_or(duration, |r| r);
    let mut chars = rest.chars();
    let out = chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default();
    (!out.is_empty()).then_some(out)
}

/// Plain text of an HTML description: tags removed, whitespace collapsed, curly quotes straightened.
fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if in_tag => {}
            '\u{2019}' => out.push('\''),
            '\u{2018}' => out.push('\''),
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_punct(tok: &str) -> &str {
    tok.trim_matches(|c: char| !c.is_alphanumeric())
}

fn is_dice(tok: &str) -> bool {
    let t = strip_punct(tok);
    match t.split_once('d') {
        Some((n, s)) => {
            !n.is_empty()
                && !s.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && s.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// First "NdM <type> damage" (type optional) in the text: `(dice, type)`.
fn first_damage(tokens: &[&str]) -> Option<(String, String)> {
    for (i, tok) in tokens.iter().enumerate() {
        if !is_dice(tok) {
            continue;
        }
        let dice = strip_punct(tok).to_string();
        let next = tokens.get(i + 1).map(|t| strip_punct(t)).unwrap_or("");
        if next.eq_ignore_ascii_case("damage") {
            return Some((dice, String::new()));
        }
        let after = tokens.get(i + 2).map(|t| strip_punct(t)).unwrap_or("");
        if after.eq_ignore_ascii_case("damage") && !next.is_empty() {
            return Some((dice, next.to_lowercase()));
        }
    }
    None
}

/// First "<Ability> saving throw": the three-letter ability.
fn first_save(tokens: &[&str]) -> Option<&'static str> {
    tokens.windows(3).find_map(|w| {
        let is_save = strip_punct(w[1]).eq_ignore_ascii_case("saving")
            && strip_punct(w[2]).eq_ignore_ascii_case("throw");
        if !is_save {
            return None;
        }
        let name = strip_punct(w[0]).to_lowercase();
        ABILITIES.iter().find(|(n, _)| *n == name).map(|(_, a)| *a)
    })
}

fn healing(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let at = lower
        .find("regains a number of hit points equal to ")
        .map(|i| i + "regains a number of hit points equal to ".len())
        .or_else(|| {
            lower
                .find("regain a number of hit points equal to ")
                .map(|i| i + "regain a number of hit points equal to ".len())
        })?;
    // `lower` and `text` share byte offsets: lower-casing ASCII-prefix text keeps lengths for these spells,
    // but guard anyway.
    let rest = text.get(at..)?;
    let clause = rest.split(['.', ';']).next().unwrap_or(rest);
    let clause = clause.replace("your spellcasting ability modifier", "mod");
    let clause = clause.trim();
    (!clause.is_empty()).then(|| format!("Heals {clause}"))
}

fn first_sentence(text: &str) -> String {
    let mut end = text.len();
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if matches!(b, b'.' | b'!' | b'?') && bytes.get(i + 1).is_none_or(|n| *n == b' ') {
            end = i + 1;
            break;
        }
    }
    text[..end].trim().to_string()
}

/// Cut to `EFFECT_MAX_CHARS` characters at a word boundary, adding an ellipsis.
fn clip(s: &str) -> String {
    if s.chars().count() <= EFFECT_MAX_CHARS {
        return s.to_string();
    }
    let cut: String = s.chars().take(EFFECT_MAX_CHARS - 1).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}\u{2026}", cut.trim_end_matches([',', ';', ':', ' ']))
}

/// Short effect line: damage and save/attack, healing, a save plus the first sentence, or the first sentence.
/// Deterministic and built only from the description text; `None` for an empty description.
#[must_use]
pub fn spell_effect(description_html: &str) -> Option<String> {
    let text = base_text(description_html);
    let text = text.as_str();
    if text.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let lower = text.to_lowercase();
    let damage =
        first_damage(&tokens).map(|(d, t)| if t.is_empty() { d } else { format!("{d} {t}") });
    let save = first_save(&tokens);
    let attack = if lower.contains("ranged spell attack") {
        Some("Ranged attack")
    } else if lower.contains("melee spell attack") {
        Some("Melee attack")
    } else {
        None
    };
    let half = lower.contains("half as much damage") || lower.contains("half damage");
    let line = match (attack, save, &damage) {
        (Some(a), _, Some(d)) => format!("{a}, {d}"),
        (None, Some(s), Some(d)) => {
            let tail = if half { ", half on save" } else { "" };
            format!("{d}, {s} save{tail}")
        }
        _ => {
            if let Some(h) = healing(text) {
                h
            } else if let Some(d) = damage {
                format!("{d} damage")
            } else if let Some(s) = save {
                format!("{s} save. {}", first_sentence(text))
            } else {
                first_sentence(text)
            }
        }
    };
    Some(clip(&line))
}

#[cfg(test)]
#[path = "spell_facts_tests.rs"]
mod tests;
