//! What an entry's text is made of, against the other entries and the source: several
//! entries holding the same text (a whole table or section copied into each of its rows), an
//! entry whose text runs into the publication's legal or credits pages, and an entry whose
//! text a picture transcription summarised instead of copying. Each fails only on evidence that
//! holds on any layout.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::super::fidelity::plain;
use super::{failed, norm, quoted_list, span};
use super::{
    DraftElement, ElementCheck, ImportDraft, DESCRIPTION_SHARED, END_MATTER_TEXT,
    TRANSCRIPTION_PARAPHRASE,
};

/// Words of a shingle compared between two descriptions.
const SHINGLE: usize = 4;
/// Fewest description words before two texts are compared (shorter ones repeat by chance:
/// "You can speak Common").
const MIN_SHARED_WORDS: usize = 12;
/// Share of the two descriptions' shingles in common (of all their shingles) that makes them
/// the same text.
const SAME_TEXT: f64 = 0.8;
/// Phrases of a publication's legal notice or credits, never of a rule.
const LEGAL_PHRASES: [&str; 14] = [
    "all rights reserved",
    "trademarks of",
    "is copyright",
    "copyright 20",
    "\u{a9}",
    "open game license",
    "community content agreement",
    "not for resale",
    "permission granted to",
    "used with permission",
    "product identity",
    "registered trademark",
    "special thanks",
    "published under",
];
/// Distinct legal phrases that make a description carry legal text (one alone can be a rule's
/// own word: a feature named "Copyright Curse").
const MIN_LEGAL_PHRASES: usize = 2;
/// Inline markers a picture transcription puts on text it summarised or reworded.
const SUMMARY_MARKERS: [&str; 4] = ["(summary)", "[summary", "(paraphrased", "[paraphrased"];
/// Words after a marker (or of a noted passage) looked for in the description.
const MARKED_WORDS: usize = 6;
/// Fewest words of a marked or noted passage looked for (fewer prove nothing).
const MIN_MARKED_WORDS: usize = 3;
/// Longest quote in a message.
const QUOTE_CHARS: usize = 80;

fn description(e: &DraftElement) -> String {
    plain(
        e.json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )
}

fn shingles(words: &[&str]) -> HashSet<String> {
    words.windows(SHINGLE).map(|w| w.join(" ")).collect()
}

/// `text` (normalised, padded) names `name` as whole words.
fn names(text: &str, name: &str) -> bool {
    let n = norm(name);
    !n.is_empty() && text.contains(&format!(" {n} "))
}

/// Elements whose description is the same text as other, differently named elements' and
/// names them, while theirs names it back: one table or section copied into each of its rows
/// (element id -> the other names). Variants cut to their own row, and a parent holding its
/// children's text, are not the same text.
pub(super) fn shared_descriptions(draft: &ImportDraft) -> HashMap<String, Vec<String>> {
    struct Entry<'a> {
        e: &'a DraftElement,
        text: String,
        shingles: HashSet<String>,
    }
    let entries: Vec<Entry> = draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null())
        .filter_map(|e| {
            let text = norm(&description(e));
            let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
            (words.len() >= MIN_SHARED_WORDS).then(|| Entry {
                e,
                shingles: shingles(&words),
                text: format!(" {text} "),
            })
        })
        .collect();
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (i, a) in entries.iter().enumerate() {
        for b in &entries[i + 1..] {
            if norm(&a.e.name) == norm(&b.e.name)
                || !names(&a.text, &b.e.name)
                || !names(&b.text, &a.e.name)
            {
                continue;
            }
            let common = a.shingles.intersection(&b.shingles).count();
            let all = a.shingles.union(&b.shingles).count();
            if all == 0 || (common as f64) < SAME_TEXT * all as f64 {
                continue;
            }
            out.entry(a.e.id.clone())
                .or_default()
                .push(b.e.name.clone());
            out.entry(b.e.id.clone())
                .or_default()
                .push(a.e.name.clone());
        }
    }
    out
}

fn shared_check(shared: &HashMap<String, Vec<String>>, e: &DraftElement) -> Vec<ElementCheck> {
    let Some(others) = shared.get(&e.id) else {
        return Vec::new();
    };
    vec![failed(
        DESCRIPTION_SHARED,
        format!(
            "Its text is the same as {} other {} ({}), and names them: it holds the whole table or section, not its own part. Cut it to its own row or text",
            others.len(),
            if others.len() == 1 { "entry's" } else { "entries'" },
            quoted_list(others)
        ),
    )]
}

/// The description carries the publication's legal notice or credits (end matter appended to
/// the entry before it, or a cover's legal block read into a root).
pub(super) fn end_matter_check(e: &DraftElement, furniture: &[String]) -> Vec<ElementCheck> {
    if super::super::passage::end_matter(&e.name) {
        return Vec::new();
    }
    // The page furniture (a running footer repeating a notice) the pack leaves out.
    let html = e
        .json
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let clean = super::super::furniture::strip_texts(html, furniture);
    let low = plain(clean.as_deref().unwrap_or(html)).to_lowercase();
    let found: Vec<&str> = LEGAL_PHRASES
        .iter()
        .copied()
        .filter(|p| low.contains(p))
        .collect();
    if found.len() < MIN_LEGAL_PHRASES {
        return Vec::new();
    }
    vec![failed(
        END_MATTER_TEXT,
        format!(
            "Its text includes the publication's legal or credits text (\"{}\", \"{}\"): that is not part of it. Cut it from the text",
            found[0], found[1]
        ),
    )]
}

/// The first (or, `last`, the final) `MARKED_WORDS` words of `text` as one normalised phrase
/// (`None` when too short).
fn phrase(text: &str, last: bool) -> Option<String> {
    let n = norm(text);
    let words: Vec<&str> = n.split(' ').filter(|w| !w.is_empty()).collect();
    if words.len() < MIN_MARKED_WORDS {
        return None;
    }
    let from = if last {
        words.len().saturating_sub(MARKED_WORDS)
    } else {
        0
    };
    Some(
        words[from..]
            .iter()
            .take(MARKED_WORDS)
            .copied()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Lines of `e`'s located passage without those of the other planned elements starting
/// inside it (their text is theirs).
pub(super) fn own_lines<'a>(
    draft: &ImportDraft,
    lines: &'a [String],
    e: &DraftElement,
) -> Vec<&'a str> {
    let Some((s, end)) = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == e.id)
        .and_then(span)
    else {
        return Vec::new();
    };
    let others: Vec<(usize, usize)> = draft
        .plan
        .elements
        .iter()
        .filter(|p| p.id != e.id)
        .filter_map(span)
        .filter(|(o, _)| *o > s && *o < end)
        .collect();
    (s..end.min(lines.len()))
        .filter(|i| !others.iter().any(|(o, oe)| i >= o && i < oe))
        .map(|i| lines[i].as_str())
        .collect()
}

/// The passages of `e` that a picture transcription summarised: the text a summary marker
/// in its own passage stands before (or, closing a sentence, after), or a passage the
/// transcription step recorded in a reading note (`transcript::NOT_VERBATIM_NOTE`), that its
/// description carries.
fn paraphrased(draft: &ImportDraft, lines: &[String], e: &DraftElement) -> Option<String> {
    let desc = format!(" {} ", norm(&description(e)));
    let carried =
        |text: &str, last: bool| phrase(text, last).filter(|o| desc.contains(&format!(" {o} ")));
    for line in own_lines(draft, lines, e) {
        // ASCII lowercase keeps byte offsets.
        let low = line.to_ascii_lowercase();
        for m in SUMMARY_MARKERS {
            let Some(at) = low.find(m) else {
                continue;
            };
            let after = line[at + m.len()..].trim_start_matches(|c: char| !c.is_alphanumeric());
            if let Some(o) = carried(after, false).or_else(|| carried(&line[..at], true)) {
                return Some(o);
            }
        }
    }
    // The passages the transcription step recorded as not copied word for word.
    let spans = super::super::ingest::transcript::not_verbatim_spans(&draft.detection.notes);
    super::super::ingest::transcript::holds_not_verbatim(&description(e), &spans).map(|passage| {
        passage
            .split_whitespace()
            .take(MARKED_WORDS)
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn paraphrase_check(draft: &ImportDraft, lines: &[String], e: &DraftElement) -> Vec<ElementCheck> {
    let Some(quote) = paraphrased(draft, lines, e) else {
        return Vec::new();
    };
    let quote: String = quote.chars().take(QUOTE_CHARS).collect();
    vec![failed(
        TRANSCRIPTION_PARAPHRASE,
        format!(
            "Part of its text (\"{quote} ...\") is a picture transcription's summary, not the printed wording. Check it against the page"
        ),
    )]
}

/// The entry checks of element `e` (not repaired by a model call: the user decides).
pub(super) fn entry_checks(
    draft: &ImportDraft,
    lines: &[String],
    shared: &HashMap<String, Vec<String>>,
    furniture: &[String],
    e: &DraftElement,
) -> Vec<ElementCheck> {
    let mut out = shared_check(shared, e);
    out.extend(end_matter_check(e, furniture));
    out.extend(paraphrase_check(draft, lines, e));
    out
}
