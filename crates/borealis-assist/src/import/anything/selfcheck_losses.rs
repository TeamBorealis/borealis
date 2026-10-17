//! Checks for losses no other check sees (wave 13): a passage that reads as short notes
//! instead of printed text (a page summarised when it was read), a printed creature type the
//! species lost, a spell the text names as learned that no rule grants, and the rows of a roll
//! table imported as options to choose. Each fails only on a shape that holds on any layout.

use serde_json::Value;

use super::super::fidelity::{plain, words};
use super::{cells, failed, span, DraftElement, ElementCheck, ImportDraft, Role};

/// Code: the element's passage is short "Label: fragment" notes, not printed prose.
pub(crate) const CONDENSED_NOTES: &str = "condensed_notes";
/// Code: the source prints a creature type the species does not carry.
pub(crate) const CREATURE_TYPE_MISSING: &str = "creature_type_missing";
/// Code: the text says a named spell is learned or cast, and no rule grants it.
pub(crate) const SPELL_NOT_GRANTED: &str = "spell_not_granted";
/// Code: a roll table's result imported as an option to choose.
pub(crate) const ROLL_TABLE_OPTION: &str = "roll_table_option";

/// Fewest "Label: fragment" bullets that make a passage notes.
const MIN_NOTE_BULLETS: usize = 4;
/// Share of a passage's body lines that are such bullets.
const NOTE_SHARE: f64 = 0.6;
/// Most words of a bullet's label.
const MAX_LABEL_WORDS: usize = 5;
/// Words of printed rules text addressing the reader; notes leave them out.
const SECOND_PERSON: [&str; 4] = ["you", "your", "yours", "youre"];

/// A "- Label: fragment" line: `Some(fragment)`.
fn note_bullet(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = ["- ", "* ", "\u{2022} "]
        .iter()
        .find_map(|b| t.strip_prefix(b))?;
    let (label, fragment) = rest.split_once(':')?;
    let label = label.trim_matches(|c: char| c == '*' || c == '_' || c.is_whitespace());
    let n = label.split_whitespace().count();
    let capital = label.chars().next().is_some_and(char::is_uppercase);
    (capital && (1..=MAX_LABEL_WORDS).contains(&n) && !fragment.trim().is_empty())
        .then_some(fragment)
}

fn addresses_reader(text: &str) -> bool {
    words(text)
        .iter()
        .any(|w| SECOND_PERSON.contains(&w.as_str()))
}

/// The element's own passage is mostly "Label: fragment" bullets that never address the reader
/// (printed traits say "you"/"your"): the page was most likely condensed into notes when it
/// was read, and the element carries the notes, not the print.
pub(super) fn condensed(own: &[&str]) -> bool {
    let body: Vec<&str> = own
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .collect();
    let notes: Vec<&str> = body.iter().filter_map(|l| note_bullet(l)).collect();
    let terse = notes.iter().filter(|f| !addresses_reader(f)).count();
    notes.len() >= MIN_NOTE_BULLETS
        && notes.len() as f64 >= NOTE_SHARE * body.len() as f64
        && terse as f64 >= NOTE_SHARE * notes.len() as f64
}

/// [`CONDENSED_NOTES`] on `e` when its own passage, or the passage of the planned element whose
/// condensed passage holds its start (a trait inside a summarised species), is notes.
fn condensed_check(draft: &ImportDraft, lines: &[String], e: &DraftElement) -> Vec<ElementCheck> {
    let own = super::entries::own_lines(draft, lines, e);
    let start = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == e.id)
        .and_then(span)
        .map(|(s, _)| s);
    let inside = start.is_some_and(|s| {
        draft.elements.iter().any(|o| {
            o.id != e.id
                && draft
                    .plan
                    .elements
                    .iter()
                    .find(|p| p.id == o.id)
                    .and_then(span)
                    .is_some_and(|(os, oe)| os < s && s < oe)
                && condensed(&super::entries::own_lines(draft, lines, o))
        })
    });
    if !condensed(&own) && !inside {
        return Vec::new();
    }
    vec![failed(
        CONDENSED_NOTES,
        "Its source text reads as short notes (\"Label: fragment\" lines), not printed text: the page was probably summarised when it was read, so wording and numbers may differ from the print. Check it against the source page".into(),
    )]
}

/// Creature types a species can print besides the default humanoid.
const CREATURE_TYPES: [&str; 13] = [
    "aberration",
    "beast",
    "celestial",
    "construct",
    "dragon",
    "elemental",
    "fey",
    "fiend",
    "giant",
    "monstrosity",
    "ooze",
    "plant",
    "undead",
];

/// Element types of a species.
const SPECIES_TYPES: [&str; 3] = ["Race", "Sub Race", "Species"];

/// The non-humanoid creature type the passage prints for the reader ("you are a construct",
/// "Creature Type: Fey").
fn printed_creature_type(own: &[&str]) -> Option<&'static str> {
    let text = words(&own.join(" ")).join(" ");
    CREATURE_TYPES.iter().copied().find(|t| {
        [
            format!("you are a {t}"),
            format!("you are an {t}"),
            format!("creature type {t}"),
            format!("creature type you are a {t}"),
            format!("creature type you are an {t}"),
        ]
        .iter()
        .any(|p| format!(" {text} ").contains(&format!(" {p} ")))
    })
}

fn creature_type_check(
    draft: &ImportDraft,
    lines: &[String],
    e: &DraftElement,
) -> Vec<ElementCheck> {
    if !SPECIES_TYPES.contains(&e.ty.as_str()) {
        return Vec::new();
    }
    let own = super::entries::own_lines(draft, lines, e);
    let Some(t) = printed_creature_type(&own) else {
        return Vec::new();
    };
    let carried = e
        .json
        .pointer("/data/creature_type")
        .and_then(Value::as_str)
        .is_some_and(|c| c.eq_ignore_ascii_case(t));
    if carried {
        return Vec::new();
    }
    vec![failed(
        CREATURE_TYPE_MISSING,
        format!("The source makes it a {t}, but its creature type is not set: the sheet treats it as humanoid"),
    )]
}

/// Words before a spell name that say the reader learns or casts it.
const LEARN_LEADS: [&str; 6] = [
    "you know the",
    "you learn the",
    "you gain the",
    "know the",
    "learn the",
    "you can cast",
];
/// Most words of a spell name.
const MAX_SPELL_WORDS: usize = 4;
/// Words in a spell name's place that name no spell ("you learn the following cantrip").
const NOT_A_NAME: [&str; 9] = [
    "following",
    "chosen",
    "same",
    "selected",
    "new",
    "additional",
    "extra",
    "one",
    "two",
];

/// Spell names the text says the reader learns ("you learn the Ember Spark cantrip") or casts
/// ("you can cast Ember Spark"), as words; for a cast, only names `known` holds.
fn learned_spells(text: &str, known: &[(String, String)]) -> Vec<String> {
    let w = words(text);
    let mut out = Vec::new();
    for lead in LEARN_LEADS {
        let lw: Vec<&str> = lead.split(' ').collect();
        for i in 0..w.len().saturating_sub(lw.len()) {
            if w[i..i + lw.len()]
                .iter()
                .map(String::as_str)
                .ne(lw.iter().copied())
            {
                continue;
            }
            let after = &w[i + lw.len()..];
            let cantrip = (1..=MAX_SPELL_WORDS)
                .find(|n| after.get(*n).is_some_and(|x| x == "cantrip"))
                .map(|n| after[..n].join(" "))
                .filter(|n| !n.split(' ').any(|w| NOT_A_NAME.contains(&w)));
            let named = cantrip.or_else(|| {
                (1..=MAX_SPELL_WORDS)
                    .rev()
                    .map(|n| after.iter().take(n).cloned().collect::<Vec<_>>().join(" "))
                    .find(|name| known.iter().any(|(k, _)| k == name))
            });
            if let Some(n) = named.filter(|n| !out.contains(n)) {
                out.push(n);
            }
        }
    }
    out
}

/// Spell ids of `{"kind": "grant", "type": "Spell", "id": ...}` rules (the grant form the
/// `spell`-field reading of `selfcheck_grants` does not see).
pub(super) fn spell_grant_ids(doc: &Value) -> Vec<String> {
    doc.get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| {
            r.get("kind").and_then(Value::as_str) == Some("grant")
                && r.get("type").and_then(Value::as_str) == Some("Spell")
        })
        .filter_map(|r| r.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// The spell names (as words) an element grants: ids' name tails ("ID_X_SPELL_EMBER_SPARK").
fn granted_names(targets: &[String]) -> Vec<String> {
    targets
        .iter()
        .map(|id| words(&id.replace('_', " ")).join(" "))
        .collect()
}

/// [`SPELL_NOT_GRANTED`]: a spell its text says is learned or cast that neither it, a planned
/// child nor its parent grants or offers (by name in a grant id) and no derive note names.
pub(super) fn spell_grant_check(
    draft: &ImportDraft,
    e: &DraftElement,
    known: &[(String, String)],
    targets_of: &dyn Fn(&str) -> Vec<String>,
    notes: &[&str],
) -> Vec<ElementCheck> {
    if matches!(e.role, Role::Text) || e.ty == "Monster" || e.ty == "Spell" {
        return Vec::new();
    }
    let text = plain(
        e.json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let named = learned_spells(&text, known);
    if named.is_empty() {
        return Vec::new();
    }
    let p = draft.plan.elements.iter().find(|p| p.id == e.id);
    let mut related: Vec<String> = vec![e.id.clone()];
    related.extend(p.and_then(|p| p.parent.clone()));
    related.extend(
        draft
            .plan
            .elements
            .iter()
            .filter(|c| c.parent.as_deref() == Some(e.id.as_str()))
            .map(|c| c.id.clone()),
    );
    let granted: Vec<String> = related
        .iter()
        .flat_map(|id| granted_names(&targets_of(id)))
        .collect();
    let noted = |n: &str| notes.iter().any(|m| words(m).join(" ").contains(n));
    let missing: Vec<&String> = named
        .iter()
        .filter(|n| !granted.iter().any(|g| g.ends_with(n.as_str())) && !noted(n))
        .collect();
    let Some(first) = missing.first() else {
        return Vec::new();
    };
    vec![failed(
        SPELL_NOT_GRANTED,
        format!("Its text says the spell \"{first}\" is learned or cast, but no rule grants it: the sheet will not list it"),
    )]
}

/// A first cell that is a roll result: "3", "1-2", "01-05", "11\u{2013}20".
fn roll_cell(cell: &str) -> bool {
    let t = cell.trim();
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '\u{2013}' | '\u{2014}' | ' '))
}

/// A die header cell: "d4", "d20", "d%", "1d6".
fn die_cell(cell: &str) -> bool {
    let t = cell.trim().to_lowercase();
    let Some((n, sides)) = t.split_once('d') else {
        return false;
    };
    n.chars().all(|c| c.is_ascii_digit())
        && !sides.is_empty()
        && (sides == "%" || sides.chars().all(|c| c.is_ascii_digit()))
}

/// Most lines above a row searched for its table's header.
const MAX_HEADER_LOOKBACK: usize = 40;

/// [`ROLL_TABLE_OPTION`]: an option of a text entry located on a table row whose first cell is
/// a roll result under a die header ("d20 | Creature"): a roll table, not a list of choices.
fn roll_option_check(draft: &ImportDraft, lines: &[String], e: &DraftElement) -> Vec<ElementCheck> {
    if e.role != Role::Option {
        return Vec::new();
    }
    let Some(p) = draft.plan.elements.iter().find(|p| p.id == e.id) else {
        return Vec::new();
    };
    let parent_is_text = p
        .parent
        .as_deref()
        .and_then(|id| draft.plan.elements.iter().find(|q| q.id == id))
        .is_some_and(|q| q.role == Role::Text && q.picks.is_none());
    let Some((s, _)) = span(p).filter(|_| parent_is_text) else {
        return Vec::new();
    };
    let Some(row) = lines.get(s).map(|l| cells(l)) else {
        return Vec::new();
    };
    if row.len() < 2 || !roll_cell(row[0]) {
        return Vec::new();
    }
    let header = (s.saturating_sub(MAX_HEADER_LOOKBACK)..s)
        .rev()
        .map(|i| cells(&lines[i]))
        .take_while(|c| c.len() >= 2)
        .any(|c| die_cell(c[0]));
    if !header {
        return Vec::new();
    }
    vec![failed(
        ROLL_TABLE_OPTION,
        "It is a result of a roll table (its row starts with a die roll), imported as an option to choose: a roll table's rows belong to what owns the table".into(),
    )]
}

/// The wave-13 loss checks of `e` (the spell grant check runs with the grant views, see
/// `selfcheck::element_checks`).
pub(super) fn loss_checks(
    draft: &ImportDraft,
    lines: &[String],
    e: &DraftElement,
) -> Vec<ElementCheck> {
    let mut out = condensed_check(draft, lines, e);
    out.extend(creature_type_check(draft, lines, e));
    out.extend(roll_option_check(draft, lines, e));
    out
}

#[cfg(test)]
#[path = "selfcheck_losses_tests.rs"]
mod tests;
