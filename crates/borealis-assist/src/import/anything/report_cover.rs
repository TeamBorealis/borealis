//! What the report may claim about a source section: a section whose text an element carries
//! (under another name, inside a root, as a sibling's row) was imported, whatever its name; a
//! skipped narrative section keeps the amounts a game needs (a magic bonus, a reward, XP) as
//! report lines.

use std::collections::HashSet;

use serde_json::Value;

use super::fidelity::{coverage, plain, words, Index};
use super::option_rows::{cells, is_heading, key_words};
use super::types::ImportDraft;

/// Share of a section body's 4-grams found in the elements' text that counts it as carried.
const CARRIED: f64 = 0.5;
/// Fewest table rows naming elements that make a table section carried.
const MIN_ELEMENT_ROWS: usize = 2;
/// Fewest words of a body measured (shorter ones are left to the name tests).
const MIN_BODY_WORDS: usize = 6;
/// Fewest words of a name that is measured as text of its own (a sentence printed as a row).
const NAME_GRAM: usize = 4;
/// Most amount lines reported per skipped section.
const MAX_AMOUNTS: usize = 10;
/// Most characters of one amount line.
const AMOUNT_CHARS: usize = 200;
/// Coin and reward words an amount is printed with ("250 gp", "500 XP").
const AMOUNT_UNITS: [&str; 12] = [
    "gp",
    "sp",
    "cp",
    "pp",
    "ep",
    "gold",
    "silver",
    "copper",
    "platinum",
    "electrum",
    "xp",
    "experience",
];

/// Heading depth of a markdown heading line.
fn depth(line: &str) -> usize {
    line.trim_start().chars().take_while(|c| *c == '#').count()
}

/// The text a source section of `name` prints, the name left out: a heading's body (to the
/// next heading of its level or higher), a table row's other cells, a run-in paragraph
/// ("Name. Text") or the lines under a bare title line.
pub(crate) fn section_body(source: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let key = key_words(name);
    if key.is_empty() {
        return None;
    }
    let title = |l: &str| key_words(l.trim_start().trim_start_matches('#'));
    // A note in brackets after the title ("Ranged Weapons (Cont.)") is no part of it.
    let bare = |l: &str| {
        key_words(
            l.trim_start()
                .trim_start_matches('#')
                .split(" (")
                .next()
                .unwrap_or(""),
        )
    };
    let headed = (0..lines.len())
        .find(|&k| is_heading(lines[k]) && title(lines[k]) == key)
        .or_else(|| (0..lines.len()).find(|&k| is_heading(lines[k]) && bare(lines[k]) == key));
    if let Some(k) = headed {
        let d = depth(lines[k]);
        let end = (k + 1..lines.len())
            .find(|&j| is_heading(lines[j]) && depth(lines[j]) <= d)
            .unwrap_or(lines.len());
        return Some(lines[k + 1..end].join("\n"));
    }
    if let Some(row) = lines.iter().find_map(|l| {
        let c = cells(l)?;
        let at = c.iter().position(|x| key_words(x) == key)?;
        Some(
            c.iter()
                .enumerate()
                .filter(|(k, _)| *k != at)
                .map(|(_, x)| *x)
                .collect::<Vec<_>>()
                .join(" "),
        )
    }) {
        return Some(row);
    }
    let k = (0..lines.len()).find(|&k| {
        let w = key_words(lines[k]);
        w.len() >= key.len() && w[..key.len()] == *key && cells(lines[k]).is_none()
    })?;
    let end = (k + 1..lines.len())
        .find(|&j| lines[j].trim().is_empty() || is_heading(lines[j]))
        .unwrap_or(lines.len());
    let first = lines[k].trim();
    let rest = first.get(name.len().min(first.len())..).unwrap_or("");
    Some(
        std::iter::once(rest)
            .chain(lines[k + 1..end].iter().copied())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Every string an element carries, as plain text.
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) if !s.starts_with("ID_") => out.push(plain(s)),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(m) => m.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// The index of everything the kept elements carry.
pub(crate) fn carried_index(draft: &ImportDraft, left_out: &HashSet<String>) -> Index {
    let mut all = Vec::new();
    for e in draft.elements.iter().filter(|e| !left_out.contains(&e.id)) {
        strings(&e.json, &mut all);
    }
    Index::new(&all.join("\n"))
}

/// The section `name` prints is in the kept elements' text (at least [`CARRIED`] of its body),
/// so a report entry saying it was not imported would be false.
pub(crate) fn carried(draft: &ImportDraft, index: &Index, name: &str) -> bool {
    let Some(body) = section_body(&draft.source_text, name) else {
        return false;
    };
    if rows_are_elements(&element_names(draft), &body) {
        return true;
    }
    let mut w = words(&body);
    // A short body (a table row printing a sentence as its name) is measured with its name.
    if w.len() < MIN_BODY_WORDS {
        let named = words(name);
        if named.len() < NAME_GRAM {
            return false;
        }
        w = named.into_iter().chain(w).collect();
    }
    coverage(&w, index, &HashSet::new()).is_some_and(|c| c >= CARRIED)
}

/// The names of the draft's generated elements, as key words.
fn element_names(draft: &ImportDraft) -> HashSet<Vec<String>> {
    draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null())
        .filter_map(|e| e.json.get("name").and_then(Value::as_str))
        .map(key_words)
        .collect()
}

/// The body is a table whose rows (header and separator left out) mostly name elements of
/// the pack: its rows were imported as those elements (a weapon table continued under a
/// "(Cont.)" caption), whatever their text.
fn rows_are_elements(names: &HashSet<Vec<String>>, body: &str) -> bool {
    let rows: Vec<Vec<&str>> = body
        .lines()
        .filter_map(cells)
        .filter(|c| c.len() > 1)
        .collect();
    let named = rows
        .iter()
        .filter(|c| {
            let first = c[0].split(" (").next().unwrap_or(c[0]);
            names.contains(&key_words(first))
        })
        .count();
    // The header row names no element; every other row is measured.
    let measured = rows.len().saturating_sub(1).max(1);
    named >= MIN_ELEMENT_ROWS && named as f64 >= CARRIED * measured as f64
}

/// A token printing a magic bonus ("+1", "+2").
fn bonus(token: &str) -> bool {
    token
        .strip_prefix('+')
        .is_some_and(|n| !n.is_empty() && n.len() <= 2 && n.chars().all(|c| c.is_ascii_digit()))
}

/// A sentence prints an amount a game needs: a magic bonus, or a number with a coin or reward
/// word after it.
fn has_amount(sentence: &str) -> bool {
    let tokens: Vec<String> = sentence
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric() && c != '+'))
        .map(str::to_lowercase)
        .collect();
    tokens.iter().enumerate().any(|(k, t)| {
        if bonus(t) {
            return true;
        }
        let number = !t.is_empty() && t.replace(',', "").chars().all(|c| c.is_ascii_digit());
        let glued = t
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == ',')
            .to_string();
        (number
            && tokens
                .get(k + 1)
                .is_some_and(|u| AMOUNT_UNITS.contains(&u.as_str())))
            || (t.starts_with(|c: char| c.is_ascii_digit())
                && AMOUNT_UNITS.contains(&glued.as_str()))
    })
}

/// The sentences of the section `name` that print an amount a game needs (one each, shortened),
/// for the report of a section that is not imported.
pub(crate) fn amounts(source: &str, name: &str) -> Vec<String> {
    let Some(body) = section_body(source, name) else {
        return Vec::new();
    };
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: Vec<String> = Vec::new();
    for s in flat.split_inclusive(['.', '!', '?']) {
        let s = s.trim();
        if has_amount(s) && !out.iter().any(|o| o == s) {
            let short: String = s.chars().take(AMOUNT_CHARS).collect();
            out.push(short);
        }
        if out.len() == MAX_AMOUNTS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "# Vault\n\n## Hall\n\nThe chest holds a +1 longsword and 250 gp in a sack. The walls are damp.\nDefeating the guard is worth 450 XP.\n\n## Next\n\nOther.\n\nSpare Coins. Ten 5gp coins.\n\n| Name | Note |\n|---|---|\n| Bell | rings twice |\n";

    #[test]
    fn a_section_body_is_found_by_heading_row_or_run_in() {
        assert!(section_body(SRC, "Hall").unwrap().contains("450 XP"));
        assert!(!section_body(SRC, "Hall").unwrap().contains("Other."));
        assert!(section_body(SRC, "Vault").unwrap().contains("Other."));
        assert_eq!(section_body(SRC, "Bell").as_deref(), Some("rings twice"));
        assert!(section_body(SRC, "Spare Coins")
            .unwrap()
            .contains("Ten 5gp coins"));
        assert!(section_body(SRC, "Absent").is_none());
    }

    #[test]
    fn amounts_of_a_skipped_section_are_one_sentence_each() {
        assert_eq!(
            amounts(SRC, "Hall"),
            vec![
                "The chest holds a +1 longsword and 250 gp in a sack.".to_string(),
                "Defeating the guard is worth 450 XP.".to_string(),
            ]
        );
        assert_eq!(
            amounts(SRC, "Spare Coins"),
            vec!["Ten 5gp coins.".to_string()]
        );
        assert!(amounts(SRC, "Bell").is_empty());
    }

    const CONT: &str = "# Arms\n\n## Thrown Arms\n\nName | Cost\n\nStone | 1 cp\n\n\
## Thrown Arms (Cont.)\n\nName | Cost | Weight\n\nBola Cord | 1 gp | 2 lb.\n\n\
Dart Pack (5) | 5 cp | 1 lb.\n\nHand Axe | 5 gp | 2 lb.\n";

    #[test]
    fn a_continued_table_whose_rows_are_elements_was_imported() {
        // The caption's note is no part of its name; the first heading of the name wins.
        assert!(section_body(CONT, "Thrown Arms").unwrap().contains("Stone"));
        let cont = CONT.replace("## Thrown Arms\n\nName | Cost\n\nStone | 1 cp\n\n", "");
        let body = section_body(&cont, "Thrown Arms").expect("the continued caption");
        assert!(body.contains("Bola Cord"));
        let names: HashSet<Vec<String>> = ["Bola Cord", "Dart Pack", "Hand Axe"]
            .iter()
            .map(|n| key_words(n))
            .collect();
        assert!(rows_are_elements(&names, &body));
        let one: HashSet<Vec<String>> = std::iter::once(key_words("Hand Axe")).collect();
        assert!(!rows_are_elements(&one, &body));
    }
}
