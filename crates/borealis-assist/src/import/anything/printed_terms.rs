//! Printed game-term wording kept in generated text. A model that knows the standard phrasing
//! writes "a Wisdom (Perception) check" where the source prints "a Perception check", or "60
//! feet" where it prints "60 ft."; the rules may use the standard terms, the text is the
//! printed text. A rewording is undone only where the passage prints the other form and never
//! the standard one.

use serde_json::Value;

use super::super::types::ImportDraft;

/// Abilities that head a skill in the standard phrasing "Wisdom (Perception)".
const ABILITIES: [&str; 6] = [
    "Strength",
    "Dexterity",
    "Constitution",
    "Intelligence",
    "Wisdom",
    "Charisma",
];
/// Monster feature lists whose entries carry printed text.
const FEATURE_LISTS: [&str; 4] = ["traits", "actions", "bonus_actions", "reactions"];

/// Case-insensitive `needle` in `hay`, not inside a longer word or number ("5 ft." is not in
/// "15 ft.").
fn has(hay: &str, needle: &str) -> bool {
    let (hay, needle) = (hay.to_lowercase(), needle.to_lowercase());
    hay.match_indices(&needle).any(|(at, _)| {
        let before = hay[..at].chars().next_back();
        let after = hay[at + needle.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric)
            && !(needle.ends_with(char::is_alphanumeric)
                && after.is_some_and(char::is_alphanumeric))
    })
}

/// `text` with each `from` (not inside a longer number or word) written as `to`. A dropped
/// abbreviation point that ended a sentence stays as the sentence's full stop, and a sentence's
/// full stop after a written-out unit is not doubled.
fn swap(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(from) {
        let before = rest[..at].chars().next_back();
        let after_text = &rest[at + from.len()..];
        out.push_str(&rest[..at]);
        if before.is_some_and(char::is_alphanumeric)
            || from.ends_with(char::is_alphanumeric)
                && after_text.starts_with(char::is_alphanumeric)
        {
            out.push_str(from);
            rest = after_text;
            continue;
        }
        out.push_str(to);
        rest = after_text;
        if to.ends_with('.') && rest.starts_with('.') {
            rest = &rest[1..];
        }
        let ends_sentence = rest.is_empty()
            || rest.starts_with('<')
            || rest
                .strip_prefix(' ')
                .is_some_and(|r| r.starts_with(char::is_uppercase));
        if from.ends_with('.') && !to.ends_with('.') && ends_sentence {
            out.push('.');
        }
    }
    out.push_str(rest);
    out
}

/// The "<Ability> (<Skill>)" phrases of `text` the passage prints as the skill alone, with the
/// skill.
fn ability_skill_fixes(text: &str, passage: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in ABILITIES {
        let mut from = 0;
        let open = format!("{a} (");
        while let Some(at) = text[from..].find(&open) {
            let start = from + at;
            let inner = start + open.len();
            let Some(close) = text[inner..].find(')') else {
                break;
            };
            let skill = &text[inner..inner + close];
            let phrase = &text[start..=inner + close];
            from = inner + close + 1;
            let plain = !skill.is_empty()
                && skill.split_whitespace().count() <= 3
                && skill.chars().all(|c| c.is_alphabetic() || c == ' ');
            if plain
                && !has(passage, phrase)
                && has(passage, skill)
                && !out.iter().any(|(p, _): &(String, String)| p == phrase)
            {
                out.push((phrase.to_string(), skill.to_string()));
            }
        }
    }
    out
}

/// "N feet" / "N ft." in `text` where the passage prints only the other form for that N.
fn distance_fixes(text: &str, passage: &str) -> Vec<(String, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out: Vec<(String, String)> = Vec::new();
    for w in words.windows(2) {
        // The number at the end of the first word ("(60", "<p>60"), the unit at the start of
        // the second ("feet.</p>").
        let digits = w[0]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == ',')
            .count();
        let n = &w[0][w[0].len() - digits..];
        let lead = w[0][..w[0].len() - digits].chars().next_back();
        if n.is_empty() || lead.is_some_and(char::is_alphanumeric) {
            continue;
        }
        let unit: String = w[1]
            .chars()
            .take_while(|c| c.is_alphabetic() || *c == '.')
            .collect();
        let unit = unit.as_str();
        let (long, short) = (format!("{n} feet"), format!("{n} ft."));
        let pair = match unit {
            "feet" | "feet." if !has(passage, &long) && has(passage, &short) => Some((long, short)),
            "ft." if !has(passage, &short) && has(passage, &long) => Some((short, long)),
            _ => None,
        };
        if let Some(p) = pair.filter(|p| !out.contains(p)) {
            out.push(p);
        }
    }
    out
}

/// `text` with its standard phrasings put back to the printed ones; the changes made.
pub(crate) fn as_printed(text: &str, passage: &str) -> (String, Vec<String>) {
    let mut out = text.to_string();
    let mut notes = Vec::new();
    let fixes = ability_skill_fixes(text, passage)
        .into_iter()
        .chain(distance_fixes(text, passage));
    for (from, to) in fixes {
        let swapped = swap(&out, &from, &to);
        if swapped != out {
            out = swapped;
            notes.push(format!("\"{from}\" written as printed: \"{to}\""));
        }
    }
    (out, notes)
}

/// The printed-text fields of an element: its description, and a monster's feature texts.
fn text_fields(json: &mut Value) -> Vec<&mut Value> {
    let mut out = Vec::new();
    let Some(obj) = json.as_object_mut() else {
        return out;
    };
    let (description, data): (Vec<_>, Vec<_>) = obj
        .iter_mut()
        .filter(|(k, _)| *k == "description" || *k == "data")
        .partition(|(k, _)| *k == "description");
    out.extend(description.into_iter().map(|(_, v)| v));
    for (_, d) in data {
        let Some(d) = d.as_object_mut() else { continue };
        for (k, list) in d.iter_mut() {
            let entries = if FEATURE_LISTS.contains(&k.as_str()) {
                list.as_array_mut()
            } else if k == "legendary" {
                list.get_mut("actions").and_then(Value::as_array_mut)
            } else {
                None
            };
            for e in entries.into_iter().flatten() {
                out.extend(e.get_mut("text"));
            }
        }
    }
    out
}

/// Every generated element's printed-text fields with standard phrasings its passage does not
/// print put back to the printed wording (noted on the element).
pub(crate) fn restore(draft: &mut ImportDraft) {
    let source = draft.source_text.clone();
    let passages: Vec<(String, String)> = draft
        .plan
        .elements
        .iter()
        .map(|p| {
            let text = super::super::source::section_text(&source, p)
                .unwrap_or_else(|| super::source_window(&source, p));
            (p.id.clone(), text)
        })
        .collect();
    for e in draft.elements.iter_mut().filter(|e| !e.json.is_null()) {
        let Some((_, passage)) = passages.iter().find(|(id, _)| *id == e.id) else {
            continue;
        };
        let mut notes = Vec::new();
        for field in text_fields(&mut e.json) {
            let Some(text) = field.as_str() else { continue };
            let (fixed, changed) = as_printed(text, passage);
            if !changed.is_empty() {
                *field = Value::String(fixed);
                notes.extend(changed);
            }
        }
        super::push_new(&mut e.normalised, notes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standard_check_phrasing_goes_back_to_the_printed_one() {
        let passage = "You have advantage on Perception checks made in fog, and darkvision 60 ft.";
        let text = "<p>You have advantage on Wisdom (Perception) checks made in fog, and darkvision 60 feet.</p>";
        let (fixed, notes) = as_printed(text, passage);
        assert_eq!(
            fixed,
            "<p>You have advantage on Perception checks made in fog, and darkvision 60 ft.</p>"
        );
        assert_eq!(notes.len(), 2, "{notes:?}");
    }

    #[test]
    fn printed_standard_phrasings_and_unprinted_terms_stay() {
        // The passage prints the standard phrasing itself.
        let passage = "Make a Wisdom (Perception) check. The light reaches 30 feet.";
        let text = "Make a Wisdom (Perception) check. The light reaches 30 feet.";
        assert_eq!(as_printed(text, passage), (text.to_string(), Vec::new()));
        // The skill is not printed at all: nothing to put back.
        let (fixed, notes) = as_printed("A Charisma (Deception) check.", "A check to lie.");
        assert_eq!(
            (fixed.as_str(), notes.len()),
            ("A Charisma (Deception) check.", 0)
        );
        // Both forms printed: left as written.
        let both = "It moves 10 feet, then 10 ft. more.";
        assert!(as_printed("It moves 10 ft.", both).1.is_empty());
        // A longer number is another distance; a written-out unit ending a sentence keeps it.
        assert!(as_printed("It moves 5 feet.", "It moves 15 ft. or 5 feet.")
            .1
            .is_empty());
        let (fixed, _) = as_printed(
            "It moves 40 ft. Then it stops; 40 ft. later",
            "It moves 40 feet. Then",
        );
        assert_eq!(fixed, "It moves 40 feet. Then it stops; 40 feet later");
    }

    #[test]
    fn monster_feature_texts_are_kept_as_printed_too() {
        let mut json = serde_json::json!({"description": "<p>Lore.</p>", "data": {
            "traits": [{"name": "Keen Smell", "text": "Advantage on Wisdom (Perception) checks that rely on smell."}],
            "legendary": {"actions": [{"name": "Dash", "text": "It moves 40 feet."}]}}});
        let passage =
            "Keen Smell. Advantage on Perception checks that rely on smell. Dash. It moves 40 ft.";
        for f in text_fields(&mut json) {
            let fixed = as_printed(f.as_str().unwrap(), passage).0;
            *f = Value::String(fixed);
        }
        assert_eq!(
            json["data"]["traits"][0]["text"],
            "Advantage on Perception checks that rely on smell."
        );
        assert_eq!(
            json["data"]["legendary"]["actions"][0]["text"],
            "It moves 40 ft."
        );
    }
}
