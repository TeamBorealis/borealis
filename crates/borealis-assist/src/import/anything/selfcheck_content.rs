//! What an element's text says against what it is: a description that is only the element's
//! name, an item whose text states a mechanic the app models but whose rules lack it, a text
//! entry that reads as an adventure's narrative, and an item the rules library already has
//! that the source only mentions. Each fails only on evidence that holds on any layout.

use serde_json::Value;

use super::super::fidelity::{plain, words};
use super::super::types::{DetectedItem, ImportKind};
use super::{failed, heading, passed, span, Checker};
use super::{
    DraftElement, ElementCheck, ImportDraft, Role, ATTUNEMENT_MISMATCH, DESCRIPTION_EMPTY,
    LIBRARY_DUPLICATE, NARRATIVE_TEXT, RULE_MISSING,
};

/// Longest quote of a description sentence in a message.
const QUOTE_CHARS: usize = 120;
/// Element types of an item (what the rules library also has).
const ITEM_TYPES: [&str; 4] = ["Magic Item", "Item", "Weapon", "Armor"];

/// One mechanic an item's text can state: phrases that state it for the item's user, and the
/// words a rule implementing it carries (in its kind, name or ids): `rule_words` start a word
/// of the rule ("immun" = "immunity"), `rule_tokens` are whole words ("ac", never "action").
struct Stated {
    what: &'static str,
    phrases: &'static [&'static str],
    rule_words: &'static [&'static str],
    rule_tokens: &'static [&'static str],
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
        rule_tokens: &[],
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
        rule_tokens: &[],
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
        rule_tokens: &[],
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
        rule_tokens: &[],
    },
    Stated {
        what: "an Armor Class bonus",
        phrases: &[
            "bonus to ac",
            "bonus to armor class",
            "to your armor class",
            "to your ac",
        ],
        rule_words: &["armorclass"],
        rule_tokens: &["ac"],
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

/// Words of a sentence that limit its mechanic to a situation the sheet cannot know (checks
/// "made to" swim, "used to" haggle): the app keeps those as text.
const SITUATION_WORDS: [&str; 5] = [
    " made to ",
    " used to ",
    " made while ",
    " involving ",
    " that involve",
];
/// Starts of the verb after "while (you)" that make it the item's own use ("while wearing
/// it"), not a situation ("while dancing").
const WEAR_VERBS: [&str; 8] = [
    "wear", "wield", "hold", "carr", "attun", "equip", "don", "bear",
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

/// The description carries no word beyond the element's name, while the element carries no
/// data or rules: the text that belongs to it was not found (a table read column by column, a
/// list of names whose text is printed elsewhere), or it is a text entry made of a section
/// heading whose text is in the entries under it. A mechanical element heading sub-entries is
/// carried by them.
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
    if extra > 0 {
        return Vec::new();
    }
    if heads_sub_entries(draft, e, &c.lines) {
        // A text entry shows nothing but its text: one that is only a heading over other
        // entries is an empty entry (a mechanical element is checked by its sub-entries).
        return if e.role == Role::Text {
            vec![failed(
                DESCRIPTION_EMPTY,
                "Its text is only its heading: the section's text is in the entries under it, so this entry shows nothing. Remove it or point to its own text".into(),
            )]
        } else {
            Vec::new()
        };
    }
    vec![failed(
        DESCRIPTION_EMPTY,
        "Its description is only its name: the text that belongs to it was not found (a table read column by column, or names whose text is printed elsewhere). Point to its text or remove it".into(),
    )]
}

// ---------------------------------------------------------------------------------------------
// An item mechanic left in the text

/// The sentence limits its mechanic to a situation: "checks made to swim", "while dancing"
/// (but "while wearing this armor" is the item's own use).
fn situational(padded: &str) -> bool {
    if SITUATION_WORDS.iter().any(|w| padded.contains(w)) {
        return true;
    }
    padded.match_indices(" while ").any(|(at, w)| {
        let mut next = padded[at + w.len()..].split_whitespace();
        let first = next.next().unwrap_or("");
        let verb = if ["you", "it", "the", "its"].contains(&first) {
            next.next().unwrap_or("")
        } else {
            first
        };
        !WEAR_VERBS.iter().any(|v| verb.starts_with(v))
    })
}

/// The words of a rule list (`"ac:armored:enhancement"` = "ac", "armored", "enhancement"),
/// and each two neighbours joined ("armor class" = "armorclass").
fn rule_word_set(el: &Value) -> Vec<String> {
    let text = el
        .get("rules")
        .map(|r| r.to_string().to_lowercase())
        .unwrap_or_default();
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let pairs = words.windows(2).map(|p| format!("{}{}", p[0], p[1]));
    words
        .iter()
        .map(|w| (*w).to_string())
        .chain(pairs)
        .collect()
}

/// Mechanics an item's description states as lasting properties for its user that none of
/// its rules carry.
fn unexpressed(el: &Value) -> Vec<(&'static str, String)> {
    let rule_words = rule_word_set(el);
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for s in sentences(&plain(str_at(el, "description"))) {
        let padded = format!(" {s} ");
        if MOMENT_WORDS.iter().any(|w| padded.contains(w)) || situational(&padded) {
            continue;
        }
        for st in &STATED {
            let stated = st.phrases.iter().any(|p| s.contains(p));
            let carried = rule_words.iter().any(|w| {
                st.rule_words.iter().any(|r| w.starts_with(r))
                    || st.rule_tokens.contains(&w.as_str())
            });
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
// Attunement

/// Values of an attunement setter the sheet reads (the restriction goes in its `addition`).
const ATTUNEMENT_VALUES: [&str; 2] = ["true", "false"];
/// Words of an item's header that say it needs attunement.
const REQUIRES_ATTUNEMENT: &str = "requires attunement";
/// Lines at the start of an item's passage that hold its header ("Armor, rare (requires
/// attunement)"); a longer passage may run into the next item's header.
const HEADER_LINES: usize = 4;

/// The item's passage header (its first non-blank lines, joined).
fn header_of(draft: &ImportDraft, e: &DraftElement) -> String {
    let Some(text) = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == e.id)
        .and_then(|p| super::super::source::section_text(&draft.source_text, p))
    else {
        return String::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .take(HEADER_LINES)
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The attunement check of an item: an attunement setter is `true` or `false` (a restriction
/// stored as its value is not read as "needs attunement"), and an item whose header says it
/// requires attunement has one that says so.
fn attunement(draft: &ImportDraft, e: &DraftElement) -> Vec<ElementCheck> {
    if !ITEM_TYPES.contains(&str_at(&e.json, "type")) {
        return Vec::new();
    }
    let setters: Vec<&Value> = e
        .json
        .get("setters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|s| str_at(s, "name") == "attunement")
        .collect();
    if let Some(odd) = setters
        .iter()
        .map(|s| {
            s.get("value").map_or_else(String::new, |v| {
                v.as_str().map_or_else(|| v.to_string(), str::to_string)
            })
        })
        .find(|v| !ATTUNEMENT_VALUES.contains(&v.trim().to_lowercase().as_str()))
    {
        let quote: String = odd.chars().take(QUOTE_CHARS).collect();
        return vec![failed(
            ATTUNEMENT_MISMATCH,
            format!("Its attunement is stored as \"{quote}\" instead of \"required\" with that restriction: the sheet may not treat it as needing attunement"),
        )];
    }
    let required = setters.iter().any(|s| str_at(s, "value") == "true");
    if !required && header_of(draft, e).contains(REQUIRES_ATTUNEMENT) {
        return vec![failed(
            ATTUNEMENT_MISMATCH,
            "Its source says it requires attunement but it does not: the sheet will apply it without attunement".into(),
        )];
    }
    Vec::new()
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
    out.extend(attunement(draft, e));
    out.extend(narrative(draft, e));
    out.extend(library_duplicate(c, draft, e));
    out
}
