//! What an element's text says against what it is: a description that is only the element's
//! name, an item whose text states a mechanic the app models but whose rules lack it, a text
//! entry that reads as an adventure's narrative, and an item the rules library already has
//! that the source only mentions. Each fails only on evidence that holds on any layout.

use serde_json::Value;

use super::super::fidelity::{plain, words};
use super::super::types::{DetectedItem, ImportKind};
use super::{failed, heading, passed, span, Checker};
use super::{
    DraftElement, ElementCheck, ImportDraft, Role, DESCRIPTION_EMPTY, LIBRARY_DUPLICATE,
    NARRATIVE_TEXT, RULE_MISSING,
};

/// Longest quote of a description sentence in a message.
const QUOTE_CHARS: usize = 120;
/// Element types of an item (what the rules library also has).
const ITEM_TYPES: [&str; 4] = ["Magic Item", "Item", "Weapon", "Armor"];

/// One mechanic an item's text can state: phrases that state it for the item's user, and the
/// words a rule implementing it carries (in its kind, name or ids).
struct Stated {
    what: &'static str,
    phrases: &'static [&'static str],
    rule_words: &'static [&'static str],
}

/// Mechanics the app models for an item's user (a rule kind exists for each).
const STATED: [Stated; 5] = [
    Stated {
        what: "advantage",
        phrases: &[
            "you have advantage on",
            "you gain advantage on",
            "gives you advantage on",
            "grants you advantage on",
            "wearer has advantage on",
            "wielder has advantage on",
            "bearer has advantage on",
        ],
        rule_words: &["advantage"],
    },
    Stated {
        what: "an immunity",
        phrases: &[
            "you are immune to",
            "you have immunity to",
            "you gain immunity to",
            "grants you immunity to",
            "wearer is immune to",
            "wielder is immune to",
            "bearer is immune to",
        ],
        rule_words: &["immun"],
    },
    Stated {
        what: "a resistance",
        phrases: &[
            "you have resistance to",
            "you gain resistance to",
            "grants you resistance to",
            "wearer has resistance to",
            "wielder has resistance to",
            "bearer has resistance to",
        ],
        rule_words: &["resist"],
    },
    Stated {
        what: "a spell",
        phrases: &[
            "you know the",
            "you learn the",
            "you can cast the",
            "you can cast it",
        ],
        rule_words: &["spell"],
    },
    Stated {
        what: "an Armor Class bonus",
        phrases: &[
            "bonus to ac",
            "bonus to armor class",
            "to your armor class",
            "to your ac",
        ],
        rule_words: &["ac", "armor"],
    },
];

/// Words of a sentence that make its mechanic a moment, not a lasting property of the item
/// (a use that spends charges, a reaction, a duration): the app keeps those as text.
const MOMENT_WORDS: [&str; 14] = [
    "charge",
    "minute",
    "hour",
    "until ",
    " when ",
    " if ",
    "once ",
    "as an action",
    "as a bonus action",
    "as a reaction",
    "for the next",
    "for 1 ",
    "for one ",
    "command word",
];

/// Phrases of a game master's narrative (an adventure's keyed area, encounter or read-aloud
/// text), never of a rule.
const NARRATIVE_PHRASES: [&str; 8] = [
    "the characters",
    "the party",
    "the adventurers",
    "the players",
    "player characters",
    "read aloud",
    "read or paraphrase",
    "boxed text",
];
/// Fewest narrative sentences that make a text entry narrative.
const MIN_NARRATIVE_SENTENCES: usize = 3;
/// Share of a text entry's sentences that must be narrative.
const NARRATIVE_SHARE: f64 = 0.3;

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn sentences(text: &str) -> Vec<String> {
    text.split(['.', '!', '?', ';', '\n'])
        .map(|s| {
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// A description that is only the name

/// Other planned elements start on deeper headings inside `e`'s passage: `e` is a section
/// heading whose text is carried by its sub-entries.
fn heads_sub_entries(draft: &ImportDraft, e: &DraftElement, lines: &[String]) -> bool {
    let Some((s, end)) = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == e.id)
        .and_then(span)
    else {
        return false;
    };
    let level = lines.get(s).and_then(|l| heading(l)).map_or(0, |(n, _)| n);
    draft
        .plan
        .elements
        .iter()
        .filter(|p| p.id != e.id)
        .filter_map(span)
        .any(|(o, _)| {
            o > s
                && o < end
                && lines
                    .get(o)
                    .and_then(|l| heading(l))
                    .is_some_and(|(n, _)| level == 0 || n > level)
        })
}

/// The description carries no word beyond the element's name, while the element carries no data
/// or rules and is no section heading of sub-entries: the text that belongs to it was not found (a table read column by
/// column, a list of names whose text is printed elsewhere).
fn description_empty(c: &Checker, draft: &ImportDraft, e: &DraftElement) -> Vec<ElementCheck> {
    if matches!(
        e.role,
        Role::Class | Role::Subclass | Role::SubclassGate | Role::Option
    ) {
        return Vec::new();
    }
    // A table row's item or a feature with rules carries itself in its data and rules.
    let carries = ["data", "rules", "setters"]
        .iter()
        .any(|k| match e.json.get(*k) {
            Some(Value::Array(a)) => !a.is_empty(),
            Some(Value::Object(m)) => !m.is_empty(),
            _ => false,
        });
    if carries {
        return Vec::new();
    }
    let name: Vec<String> = words(&e.name);
    let extra = words(&plain(str_at(&e.json, "description")))
        .into_iter()
        .filter(|w| !name.contains(w))
        .count();
    if extra > 0 || heads_sub_entries(draft, e, &c.lines) {
        return Vec::new();
    }
    vec![failed(
        DESCRIPTION_EMPTY,
        "Its description is only its name: the text that belongs to it was not found (a table read column by column, or names whose text is printed elsewhere). Point to its text or remove it".into(),
    )]
}

// ---------------------------------------------------------------------------------------------
// An item mechanic left in the text

/// Mechanics an item's description states as lasting properties for its user that none of
/// its rules carry.
fn unexpressed(el: &Value) -> Vec<(&'static str, String)> {
    let rules = el
        .get("rules")
        .map(|r| r.to_string().to_lowercase())
        .unwrap_or_default();
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for s in sentences(&plain(str_at(el, "description"))) {
        let padded = format!(" {s} ");
        if MOMENT_WORDS.iter().any(|w| padded.contains(w)) {
            continue;
        }
        for st in &STATED {
            let stated = st.phrases.iter().any(|p| s.contains(p));
            let carried = st.rule_words.iter().any(|w| rules.contains(w));
            if stated && !carried && !out.iter().any(|(w, _)| *w == st.what) {
                out.push((st.what, s.clone()));
            }
        }
    }
    out
}

/// The rule-missing check of an item: every lasting mechanic its text states is a rule.
fn rule_missing(e: &DraftElement) -> Vec<ElementCheck> {
    if !ITEM_TYPES.contains(&str_at(&e.json, "type")) {
        return Vec::new();
    }
    let found = unexpressed(&e.json);
    let Some((what, quote)) = found.first() else {
        return vec![passed(
            RULE_MISSING,
            "Every mechanic its text states is a rule",
        )];
    };
    let quote: String = quote.chars().take(QUOTE_CHARS).collect();
    vec![failed(
        RULE_MISSING,
        format!(
            "Its text gives {what} (\"{quote}\") but no rule does: the sheet will not apply it{}",
            match found.len() {
                1 => String::new(),
                n => format!(" ({} more like it)", n - 1),
            }
        ),
    )]
}

// ---------------------------------------------------------------------------------------------
// Narrative imported as rules

/// The detection called the entry narrative, or its text is mostly a game master's narrative.
fn narrative(draft: &ImportDraft, e: &DraftElement) -> Vec<ElementCheck> {
    if e.role != Role::Text {
        return Vec::new();
    }
    let key = super::norm(&e.name);
    let detected = draft
        .detection
        .other
        .iter()
        .any(|o| super::norm(&o.name) == key && super::super::intake::is_narrative(&o.what));
    let all = sentences(&plain(str_at(&e.json, "description")));
    let told = all
        .iter()
        .filter(|s| NARRATIVE_PHRASES.iter().any(|p| s.contains(p)))
        .count();
    let reads_narrative =
        told >= MIN_NARRATIVE_SENTENCES && told as f64 >= NARRATIVE_SHARE * all.len() as f64;
    if !detected && !reads_narrative {
        return Vec::new();
    }
    vec![failed(
        NARRATIVE_TEXT,
        "It reads as an adventure's narrative (what happens to the characters), not as a rule: it may not belong in the pack".into(),
    )]
}

// ---------------------------------------------------------------------------------------------
// An official item re-created

/// An item the rules library has under the same name that the source only mentions (it prints
/// no definition of its own).
fn library_duplicate(c: &Checker, draft: &ImportDraft, e: &DraftElement) -> Vec<ElementCheck> {
    if !ITEM_TYPES.contains(&e.ty.as_str()) {
        return Vec::new();
    }
    let item = DetectedItem {
        kind: ImportKind::Item,
        name: e.name.clone(),
        parent_hint: None,
        item_type: None,
    };
    let found = super::super::intake::library_references(
        &c.library,
        draft.edition,
        std::slice::from_ref(&item),
        &draft.source_text,
    );
    found
        .first()
        .map(|(name, id)| {
            vec![failed(
                LIBRARY_DUPLICATE,
                format!(
                    "The rules already have \"{name}\" ({id}) and the source only mentions it: this is probably a copy of the official item, not a new one"
                ),
            )]
        })
        .unwrap_or_default()
}

/// The content checks of element `e` (not repaired by a model call: the user decides).
pub(super) fn content_checks(
    c: &Checker,
    draft: &ImportDraft,
    e: &DraftElement,
) -> Vec<ElementCheck> {
    let mut out = description_empty(c, draft, e);
    out.extend(rule_missing(e));
    out.extend(narrative(draft, e));
    out.extend(library_duplicate(c, draft, e));
    out
}
