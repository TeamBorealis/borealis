//! Opening lore a generated root left out: the printed paragraphs at the head of its own
//! passage (before the first one its description carries) that no description of the import
//! holds are put back, as printed, at the head of the root's description.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::super::fidelity::{coverage, plain, words, Index, MIN_COVERAGE};
use super::super::types::{ImportDraft, Role};
use super::{content, html_of, paragraphs, share, starts, MIN_PARAGRAPH_WORDS};

/// Most share of a left-out paragraph's content words its root's description may hold (more
/// means the description rewords it).
const MAX_REWORDED: f64 = 0.5;

/// Most words of a field label opening a line ("Casting Time:").
const MAX_LABEL_WORDS: usize = 3;

/// A paragraph of running prose: it ends a sentence and no line opens with a field label (a
/// stat block's "Range: 30 feet" is the element's data, not its lore).
fn prose(text: &str) -> bool {
    let labelled = text.lines().any(|l| {
        l.split_once(':').is_some_and(|(label, _)| {
            let n = label.split_whitespace().count();
            (1..=MAX_LABEL_WORDS).contains(&n) && label.trim_start().starts_with(char::is_uppercase)
        })
    });
    text.trim_end().ends_with(['.', '!', '?']) && !labelled
}

/// Roots whose generated description should open with their printed lore.
fn lore_root(role: Role) -> bool {
    matches!(role, Role::Standalone | Role::Class | Role::Subclass)
}

/// Puts each generated root's left-out opening lore back (see the module doc). Returns the ids
/// of the elements changed.
pub(crate) fn restore_opening_lore(draft: &mut ImportDraft) -> Vec<String> {
    let all: String = draft
        .elements
        .iter()
        .filter_map(|e| e.json.get("description").and_then(Value::as_str))
        .map(plain)
        .collect::<Vec<_>>()
        .join("\n");
    let everywhere = Index::new(&all);
    let none = HashSet::new();
    let paras = paragraphs(&draft.source_text);
    let mut changed = Vec::new();
    for i in 0..draft.elements.len() {
        let e = &draft.elements[i];
        if e.json.is_null() || e.revisions > 0 || !lore_root(e.role) {
            continue;
        }
        let Some(ex) = draft
            .plan
            .elements
            .iter()
            .find(|p| p.id == e.id)
            .and_then(|p| p.excerpt.as_ref())
        else {
            continue;
        };
        let s = ex.start_line.saturating_sub(1);
        let next = starts(&draft.plan, &e.id)
            .into_iter()
            .filter(|l| *l > s)
            .min()
            .unwrap_or(usize::MAX);
        let end = ex.end_line.min(next).max(s + 1);
        let desc = e
            .json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let own = Index::new(&plain(&desc));
        let have = content(&plain(&desc));
        let mut missing: Vec<&str> = Vec::new();
        let mut carried = false;
        for (_, _, text) in paras.iter().filter(|(l, ..)| *l >= s && *l < end) {
            let w = words(text);
            if w.len() < MIN_PARAGRAPH_WORDS {
                continue;
            }
            if coverage(&w, &own, &none).is_some_and(|c| c >= MIN_COVERAGE) {
                carried = true;
                break;
            }
            let elsewhere = coverage(&w, &everywhere, &none).is_some_and(|c| c >= MIN_COVERAGE);
            if !elsewhere && prose(text) && share(&content(text), &have) < MAX_REWORDED {
                missing.push(text);
            }
        }
        // Only lore ahead of text the description does carry: a description that carries
        // none of its passage is a rewording, not a cut.
        if !carried || missing.is_empty() {
            continue;
        }
        let lines: Vec<String> = missing
            .iter()
            .flat_map(|p| p.lines().chain(std::iter::once("")))
            .map(str::to_string)
            .collect();
        let restored = format!("{}{desc}", html_of(&lines));
        let n = missing.len();
        let e = &mut draft.elements[i];
        e.json["description"] = json!(restored);
        e.normalised.push(format!(
            "description: {n} printed opening paragraph(s) the generated text left out added"
        ));
        changed.push(e.id.clone());
    }
    changed
}
