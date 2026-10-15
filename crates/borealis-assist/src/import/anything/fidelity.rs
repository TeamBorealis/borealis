//! Description fidelity: an imported description is the source's own text, the way the
//! homebrew packs of the reference app keep it. Every description sentence must be traceable
//! to the source by word 4-grams; one that is not gets one repair, then a review note. Plan
//! briefs never tell generation to leave source lines out.

use std::collections::HashSet;

use serde_json::Value;

use super::sweep::SweepIssue;
use super::types::{DraftElement, ElementStatus, ImportDraft, Role};

/// Words per n-gram of the overlap measure.
const GRAM: usize = 4;
/// Shorter sentences are not measured (labels, run-in names, "See below.").
const MIN_SENTENCE_WORDS: usize = 6;
/// A sentence with less of its n-grams in the source is not the source's text.
pub(crate) const MIN_COVERAGE: f64 = 0.5;
/// Fewest words of a printed sentence in an element's located passage for a fidelity repair.
const MIN_PROSE_WORDS: usize = 8;
/// Most elements repaired for fidelity in one import (the rest get the review note only).
const MAX_REPAIRS: usize = 12;
/// Most sentences quoted in one issue.
const MAX_QUOTED: usize = 3;
/// Characters of a quoted sentence.
const QUOTE_CHARS: usize = 160;

/// Prefix of the issue the sweep raises (and of the review note left after the repair).
const ISSUE: &str = "description sentences not found in the source";
const NOTE: &str = "description not traced to the source after a repair";

/// Short words never masked as part of a name.
const STOP: [&str; 24] = [
    "a", "an", "the", "of", "to", "and", "or", "in", "on", "at", "by", "for", "with", "you",
    "your", "is", "are", "be", "can", "as", "it", "its", "from", "into",
];

/// The text of an HTML fragment: tags become spaces, the common entities are decoded.
pub(crate) fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => {
                tag = false;
                out.push(' ');
            }
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Lowercase word tokens: apostrophes dropped ("can't" = "cant"), a line-end hyphen joined,
/// digits apart from letters ("25gp" = "25 gp").
pub(crate) fn words(text: &str) -> Vec<String> {
    tokens(text, true)
}

/// [`words`], with a line-end hyphen joining the word (`join`) or kept as a hyphen (a
/// compound printed across a line break, "rainbow-\ncolored" = "rainbow colored").
fn tokens(text: &str, join: bool) -> Vec<String> {
    let text = text.replace(['\u{2019}', '\u{2018}', '\''], "");
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    while let Some(c) = chars.next() {
        if c == '-' && join && !cur.is_empty() {
            // "de-\n termining": a hyphen at a line end joins the word.
            let mut look = chars.clone();
            let mut newline = false;
            while let Some(n) = look.peek().copied().filter(|n| n.is_whitespace()) {
                newline |= n == '\n';
                look.next();
            }
            if newline && look.peek().is_some_and(|n| n.is_lowercase()) {
                chars = look;
                continue;
            }
        }
        if !c.is_alphanumeric() {
            flush(&mut cur, &mut out);
            continue;
        }
        let switch = cur
            .chars()
            .last()
            .is_some_and(|l| l.is_ascii_digit() != c.is_ascii_digit());
        if switch {
            flush(&mut cur, &mut out);
        }
        cur.extend(c.to_lowercase());
    }
    flush(&mut cur, &mut out);
    out
}

fn grams(w: &[String]) -> impl Iterator<Item = String> + '_ {
    w.windows(GRAM).map(|g| g.join(" "))
}

/// The word 4-grams of a text, for [`coverage`].
pub(crate) struct Index(HashSet<String>);

impl Index {
    /// A line-end hyphen is a word break or a compound's own hyphen: both readings are indexed,
    /// so "de-\ntermining" matches "determining" and "rainbow-\ncolored" "rainbow-colored".
    pub(crate) fn new(text: &str) -> Self {
        let joined = words(text);
        let kept = tokens(text, false);
        Self(grams(&joined).chain(grams(&kept)).collect())
    }
}

/// Share of the sentence's 4-grams found in `index`, n-grams with a masked word not counted;
/// `None` when nothing is left to count.
pub(crate) fn coverage(sentence: &[String], index: &Index, mask: &HashSet<String>) -> Option<f64> {
    let counted: Vec<&[String]> = sentence
        .windows(GRAM)
        .filter(|g| !g.iter().any(|w| mask.contains(w)))
        .collect();
    if counted.is_empty() {
        return None;
    }
    let found = counted
        .iter()
        .filter(|g| index.0.contains(&g.join(" ")))
        .count();
    Some(found as f64 / counted.len() as f64)
}

/// The words of every planned element's name: the pipeline names elements (a rename, a core
/// element the source calls otherwise), so a name is never evidence of rewording.
pub(crate) fn name_mask(draft: &ImportDraft) -> HashSet<String> {
    draft
        .plan
        .elements
        .iter()
        .map(|p| p.name.as_str())
        .chain(draft.plan.groups.iter().map(|g| g.name.as_str()))
        .flat_map(words)
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect()
}

/// Paragraphs the pipeline writes itself: table columns kept as "<strong>Label:</strong>
/// value" lines and the attunement line from the setter.
pub(crate) fn pipeline_paragraph(html: &str) -> bool {
    let t = html.trim_start();
    (t.starts_with("<p><strong>") && t.contains(":</strong>"))
        || plain(t).trim_start().starts_with("Requires attunement")
}

/// Tags that end a line of their own: a list item, a heading and every table cell (a table's
/// cells are measured one by one, so a table re-laid in another row order, such as a d12 table
/// printed in three column pairs, is still the source's text).
const LINE_ENDS: [&str; 13] = [
    "<br>", "<br/>", "<br />", "</li>", "</td>", "</th>", "</tr>", "</h1>", "</h2>", "</h3>",
    "</h4>", "</h5>", "</h6>",
];

/// The sentences of a description, paragraph by paragraph, without the pipeline's own.
pub(crate) fn sentences(description: &str) -> Vec<String> {
    let mut out = Vec::new();
    for para in description.split_inclusive("</p>") {
        if pipeline_paragraph(para) {
            continue;
        }
        let text = plain(&LINE_ENDS.iter().fold(para.to_string(), |t, end| {
            t.replace(end, &format!("{end}\n"))
        }));
        for line in text.split('\n') {
            let mut start = 0;
            let b = line.as_bytes();
            for (i, c) in line.char_indices() {
                let end = i + c.len_utf8();
                if matches!(c, '.' | '!' | '?') && b.get(end).is_some_and(u8::is_ascii_whitespace) {
                    out.push(line[start..end].trim().to_string());
                    start = end;
                }
            }
            out.push(line[start..].trim().to_string());
        }
    }
    out.retain(|s| !s.is_empty());
    out
}

/// Description sentences of `el` with less than [`MIN_COVERAGE`] of their 4-grams in `index`.
pub(crate) fn untraced(el: &Value, index: &Index, mask: &HashSet<String>) -> Vec<String> {
    let desc = el.get("description").and_then(Value::as_str).unwrap_or("");
    sentences(desc)
        .into_iter()
        .filter(|s| {
            let w = words(s);
            w.len() >= MIN_SENTENCE_WORDS
                && coverage(&w, index, mask).is_some_and(|c| c < MIN_COVERAGE)
        })
        .collect()
}

/// Elements whose description is checked: generated ones the user has not revised. A subclass
/// gate is written by the pipeline, and so is a located text-only element (its passage as
/// printed); an unlocated one is written by the model from the whole source and is checked.
fn checked(draft: &ImportDraft, e: &DraftElement) -> bool {
    let pipeline_text = e.role == Role::Text && located(draft, &e.id);
    !e.json.is_null()
        && e.status != ElementStatus::Failed
        && e.revisions == 0
        && e.role != Role::SubclassGate
        && !pipeline_text
}

/// The planned element `id` has a source passage.
fn located(draft: &ImportDraft, id: &str) -> bool {
    draft
        .plan
        .elements
        .iter()
        .any(|p| p.id == id && p.excerpt.is_some())
}

fn quoted(found: &[String]) -> String {
    found
        .iter()
        .take(MAX_QUOTED)
        .map(|s| {
            let cut: String = s.chars().take(QUOTE_CHARS).collect();
            format!("\"{cut}\"")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Elements whose description has sentences the source does not, with those sentences.
pub(crate) fn findings(draft: &ImportDraft) -> Vec<(String, Vec<String>)> {
    let index = Index::new(&draft.source_text);
    let mask = name_mask(draft);
    draft
        .elements
        .iter()
        .filter(|e| checked(draft, e))
        .map(|e| (e.id.clone(), untraced(&e.json, &index, &mask)))
        .filter(|(_, f)| !f.is_empty())
        .collect()
}

/// A fidelity repair can help and is worth its call: the element has printed text to copy and
/// is not a class root (whose repair regenerates its whole progression for a wording fix).
fn repairable(draft: &ImportDraft, id: &str) -> bool {
    draft.element(id).is_some_and(|e| e.role != Role::Class) && has_printed_text(draft, id)
}

/// The element's located passage has a printed sentence to copy (a repair can only help then;
/// a passage of headings and labels has none).
fn has_printed_text(draft: &ImportDraft, id: &str) -> bool {
    draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| super::source::section_text(&draft.source_text, p))
        .is_some_and(|t| {
            let prose: String = t
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n");
            prose
                .split(['.', '!', '?'])
                .any(|s| words(s).len() >= MIN_PROSE_WORDS)
        })
}

/// Sentences no source passage has, as repairable issues (at most [`MAX_REPAIRS`] elements, and
/// only [`repairable`] ones; the rest get the review note without a call).
pub(crate) fn sweep(draft: &ImportDraft) -> Vec<SweepIssue> {
    findings(draft)
        .into_iter()
        .filter(|(id, _)| repairable(draft, id))
        .take(MAX_REPAIRS)
        .map(|(id, found)| SweepIssue {
            level: 0,
            element_id: Some(id),
            message: format!(
                "{ISSUE} (write the description in the source's printed wording, sentence by sentence; add no summary, lead-in or sentence of your own): {}",
                quoted(&found)
            ),
        })
        .collect()
}

/// After the repair: what is still not the source's text is a review note on its element.
pub(crate) fn note_leftovers(draft: &mut ImportDraft) {
    for (id, found) in findings(draft) {
        let Some(e) = draft.elements.iter_mut().find(|e| e.id == id) else {
            continue;
        };
        let msg = format!("{NOTE}: {}", quoted(&found));
        e.issues.retain(|i| !i.starts_with(NOTE));
        e.issues.push(msg);
        if matches!(e.status, ElementStatus::Ok | ElementStatus::Repaired) {
            e.status = ElementStatus::NeedsReview;
        }
    }
}

/// Words that make a brief sentence an instruction to leave source text out.
const OMIT_WORDS: [&str; 9] = [
    "left out",
    "leave out",
    "leaving out",
    "omit",
    "not included",
    "excluded",
    "not written",
    "not imported",
    "dropped",
];
/// What such a sentence leaves out: source text, not a game object.
const TEXT_WORDS: [&str; 10] = [
    "line",
    "text",
    "section",
    "paragraph",
    "description",
    "rule",
    "entry",
    "table",
    "trait",
    "score",
];

/// A plan brief without sentences telling generation to leave source lines out (a brief is a
/// summary; it never removes source content).
pub(crate) fn sanitize_brief(brief: &str) -> String {
    let omits = |s: &str| {
        let low = s.to_lowercase();
        OMIT_WORDS.iter().any(|w| low.contains(w)) && TEXT_WORDS.iter().any(|w| low.contains(w))
    };
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0;
    let b = brief.as_bytes();
    for (i, c) in brief.char_indices() {
        let end = i + c.len_utf8();
        if matches!(c, '.' | '!' | '?') && b.get(end).map_or(true, u8::is_ascii_whitespace) {
            out.push(&brief[start..end]);
            start = end;
        }
    }
    out.push(&brief[start..]);
    out.into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !omits(s))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "fidelity_tests.rs"]
pub(crate) mod tests;
