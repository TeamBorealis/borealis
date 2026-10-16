//! Deterministic text fixes made before a draft becomes a pack: the options of a rule printed
//! as table rows carry their own row and nothing else, the rule carries the pick of them, and
//! the publication's legal or trademark notice is no part of a rules or element text.

use serde_json::{json, Value};

use super::fidelity::plain;
use super::option_rows::{blocks as row_blocks, find_rows, row_html};
use super::types::{DraftElement, ImportDraft, PlannedElement, Role};

/// Phrases of a legal or trademark notice (lowercase); a paragraph with one is the notice.
const LEGAL_PHRASES: [&str; 8] = [
    "trademark",
    "copyright",
    "\u{a9}",
    "all rights reserved",
    "open game license",
    "community content agreement",
    "used under license",
    "is licensed under",
];

/// The draft with the fixes made (each noted on its element).
pub(crate) fn prepare(draft: &ImportDraft) -> ImportDraft {
    let mut out = draft.clone();
    row_entries(&mut out);
    row_options(&mut out);
    rule_selects(&mut out);
    for e in out.elements.iter_mut() {
        strip_legal(e);
    }
    out
}

fn planned<'a>(draft: &'a ImportDraft, id: &str) -> Option<&'a PlannedElement> {
    draft.plan.elements.iter().find(|p| p.id == id)
}

/// The option elements of a text-only rule (its table of options, see
/// `intake::attach_option_tables`).
fn rule_options(draft: &ImportDraft) -> Vec<(String, String, Vec<String>)> {
    draft
        .plan
        .elements
        .iter()
        .filter(|p| p.role == Role::Text && p.picks.is_some())
        .map(|p| {
            let options = draft
                .plan
                .elements
                .iter()
                .filter(|c| c.parent.as_deref() == Some(p.id.as_str()) && c.role == Role::Option)
                .map(|c| c.name.clone())
                .collect();
            (p.id.clone(), p.name.clone(), options)
        })
        .collect()
}

/// Fewest text entries printed as the rows of one table that are described row by row.
const MIN_ROW_ENTRIES: usize = 3;

/// Text entries that are the named rows of one printed table (at least [`MIN_ROW_ENTRIES`],
/// each located on its own row or nowhere) are described by their own row, never by the rest
/// of the table and whatever follows it up to the next heading.
fn row_entries(draft: &mut ImportDraft) {
    let texts: Vec<(String, String, Option<usize>)> = draft
        .plan
        .elements
        .iter()
        .filter(|p| p.role == Role::Text && p.picks.is_none())
        .map(|p| {
            let at = p.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1));
            (p.id.clone(), p.name.clone(), at)
        })
        .collect();
    let names: Vec<String> = texts.iter().map(|(_, n, _)| n.clone()).collect();
    let rows = find_rows(&draft.source_text, &names);
    for block in row_blocks(&draft.source_text, &rows) {
        if block.len() < MIN_ROW_ENTRIES {
            continue;
        }
        for k in block {
            let (Some(row), (id, name, at)) = (rows[k].as_ref(), &texts[k]) else {
                continue;
            };
            if at.is_some_and(|a| a != row.line) {
                continue;
            }
            let (html, _) = row_html(&draft.source_text, name, row);
            let Some(e) = draft
                .elements
                .iter_mut()
                .find(|e| e.id == *id && !e.json.is_null())
            else {
                continue;
            };
            if e.json.get("description").and_then(Value::as_str) != Some(html.as_str()) {
                e.json["description"] = json!(html);
                e.normalised
                    .push("description: its own table row only, not the whole table".into());
            }
        }
    }
}

/// Each option of a rule's options table is described by its own row only: never the whole
/// table, never effect text the source does not print (a row printing only the name gets no
/// rules either).
fn row_options(draft: &mut ImportDraft) {
    for (owner, _, names) in rule_options(draft) {
        let rows = find_rows(&draft.source_text, &names);
        for (name, row) in names.iter().zip(rows) {
            let Some(row) = row else { continue };
            let (html, effect) = row_html(&draft.source_text, name, &row);
            let Some(e) = draft.elements.iter_mut().find(|e| {
                e.name == *name
                    && e.role == Role::Option
                    && !e.json.is_null()
                    && draft
                        .plan
                        .elements
                        .iter()
                        .any(|p| p.id == e.id && p.parent.as_deref() == Some(owner.as_str()))
            }) else {
                continue;
            };
            let Some(o) = e.json.as_object_mut() else {
                continue;
            };
            let same = o.get("description").and_then(Value::as_str) == Some(html.as_str());
            o.insert("description".into(), json!(html));
            let had_rules = o
                .get("rules")
                .and_then(Value::as_array)
                .is_some_and(|r| !r.is_empty());
            if !effect && had_rules {
                o.insert("rules".into(), json!([]));
                e.normalised.push(
                    "rules removed: the source prints only this option's name, no effect".into(),
                );
            }
            if !same {
                e.normalised
                    .push("description: its own row of the options table only".into());
            }
        }
    }
}

/// A rule whose options are elements carries the pick of them: one select of the options'
/// type and `supports` key, the number as printed.
fn rule_selects(draft: &mut ImportDraft) {
    let owners = rule_options(draft);
    for (id, name, _) in owners {
        let Some(picks) = planned(draft, &id).and_then(|p| p.picks.clone()) else {
            continue;
        };
        let offered = draft.elements.iter().any(|e| {
            !e.json.is_null()
                && planned(draft, &e.id).is_some_and(|p| p.parent.as_deref() == Some(id.as_str()))
        });
        let Some(e) = draft
            .elements
            .iter_mut()
            .find(|e| e.id == id && !e.json.is_null())
        else {
            continue;
        };
        if !offered {
            continue;
        }
        let selects: Vec<Value> = picks
            .choose
            .iter()
            .map(|c| {
                let mut s = json!({"kind": "select", "type": picks.ty, "name": name,
                    "supports": picks.key});
                if c.count > 1 {
                    s["number"] = json!(c.count);
                }
                s
            })
            .collect();
        let count = picks.choose.first().map_or(1, |c| c.count);
        if let Some(o) = e.json.as_object_mut() {
            o.insert("rules".into(), Value::Array(selects));
            e.normalised.push(format!(
                "offers its options as a select of {count} (supports \"{}\")",
                picks.key
            ));
        }
    }
}

/// A paragraph of the publication's legal or trademark notice.
fn legal(paragraph: &str) -> bool {
    let low = plain(paragraph).to_lowercase();
    LEGAL_PHRASES.iter().any(|p| low.contains(p))
}

/// The top-level blocks of a description (`<p>`, `<table>`, `<ul>` ...), `None` when it has
/// text outside them.
fn blocks(html: &str) -> Option<Vec<&str>> {
    let mut out = Vec::new();
    let mut rest = html.trim();
    while !rest.is_empty() {
        let tag: String = rest
            .strip_prefix('<')?
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if tag.is_empty() {
            return None;
        }
        let close = format!("</{tag}>");
        let end = rest.find(&close)? + close.len();
        out.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    Some(out)
}

/// The legal or trademark notice paragraphs leave an element's description, unless the element
/// is that notice (end matter by name, or nothing but the notice).
fn strip_legal(e: &mut DraftElement) {
    if super::passage::end_matter(&e.name) {
        return;
    }
    let Some(d) = e.json.get("description").and_then(Value::as_str) else {
        return;
    };
    let Some(parts) = blocks(d) else { return };
    let kept: Vec<&str> = parts.iter().copied().filter(|p| !legal(p)).collect();
    let dropped = parts.len() - kept.len();
    if dropped == 0 || kept.iter().all(|p| plain(p).trim().is_empty()) {
        return;
    }
    let text = kept.concat();
    e.json["description"] = json!(text);
    e.normalised.push(format!(
        "description: the publication's legal or trademark notice left out ({dropped} paragraph(s))"
    ));
}

#[cfg(test)]
#[path = "text_fixes_tests.rs"]
mod tests;
