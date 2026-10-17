//! Deterministic text fixes made before a draft becomes a pack: the options of a rule printed
//! only as table rows carry their own row and nothing else (an option printed with a passage of
//! its own keeps it), the rule carries the pick of them, an emphasised transcription marker
//! leaves whole, and the publication's legal or trademark notice, with the imprint printed
//! with it, is no part of a rules or element text.

use serde_json::{json, Value};

use super::fidelity::plain;
use super::intake::printed_apart;
use super::option_rows::{blocks as row_blocks, find_rows, row_html, Row};
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
        strip_wrapped_markers(e);
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

/// The row is all the source prints for `name`: no passage of its own elsewhere (a section
/// or a paragraph, see [`printed_apart`]), which always wins over the row.
fn row_only(source: &str, name: &str, row: &Row) -> bool {
    !printed_apart(source, name, row.line, row.end)
}

/// Text entries that are the named rows of one printed table (at least [`MIN_ROW_ENTRIES`],
/// each located on its own row or nowhere, and printed nowhere else) are described by their
/// own row, never by the rest of the table and whatever follows it up to the next heading.
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
            if at.is_some_and(|a| a != row.line) || !row_only(&draft.source_text, name, row) {
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

/// Each option of a rule's options table that the source prints only as its row is described
/// by that row only: never the whole table, never effect text the source does not print (a
/// row printing only the name gets no rules either). An option with a passage of its own (its
/// section or paragraph) keeps its description from that passage.
fn row_options(draft: &mut ImportDraft) {
    for (owner, _, names) in rule_options(draft) {
        let rows = find_rows(&draft.source_text, &names);
        for (name, row) in names.iter().zip(rows) {
            let Some(row) = row.filter(|r| row_only(&draft.source_text, name, r)) else {
                continue;
            };
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

/// Emphasis a transcription wraps a whole marker in ("*(Summary)*", "<em>[Paraphrased]</em>"),
/// longest first.
const MARKER_WRAPS: [(&str, &str); 9] = [
    ("***", "***"),
    ("**", "**"),
    ("__", "__"),
    ("*", "*"),
    ("_", "_"),
    ("<strong>", "</strong>"),
    ("<em>", "</em>"),
    ("<b>", "</b>"),
    ("<i>", "</i>"),
];

/// `html` without the bare transcription markers ("(Summary)", "[Paraphrased]") printed inside
/// emphasis of their own: the marker and its emphasis go together, so no empty emphasis
/// ("** ") is left behind. A marker around text ("[Summary: text]") keeps the text, and so its
/// emphasis (`furniture::strip_markers` takes those). `None` when there is none.
fn without_wrapped_markers(html: &str) -> Option<String> {
    let (mut out, mut rest, mut changed) = (String::with_capacity(html.len()), html, false);
    while let Some(k) = rest.find(['(', '[']) {
        let close = if rest[k..].starts_with('(') { ')' } else { ']' };
        let wrapped = rest[k + 1..]
            .find(close)
            .map(|e| k + 1 + e)
            .filter(|e| !rest[k + 1..*e].contains(['<', '(', '[']))
            .filter(|e| super::furniture::strip_markers(&rest[k..=*e]).as_deref() == Some(""))
            .and_then(|e| {
                MARKER_WRAPS
                    .iter()
                    .find(|(o, c)| rest[..k].ends_with(o) && rest[e + 1..].starts_with(c))
                    .map(|(o, c)| (k - o.len(), e + 1 + c.len()))
            });
        let Some((from, to)) = wrapped else {
            out.push_str(&rest[..=k]);
            rest = &rest[k + 1..];
            continue;
        };
        out.push_str(&rest[..from]);
        rest = &rest[to..];
        if out.is_empty() || out.ends_with([' ', '>']) {
            rest = rest.strip_prefix(' ').unwrap_or(rest);
        }
        changed = true;
    }
    out.push_str(rest);
    changed.then(|| out.replace(" </p>", "</p>").replace("<p></p>", ""))
}

/// Markdown emphasis delimiters a stripped marker leaves at the start of a text, longest first.
const ORPHAN_DELIMITERS: [&str; 3] = ["***", "**", "__"];

/// Tags that open a text of their own (a paragraph, item, cell, heading or line break).
const BLOCK_OPENERS: [&str; 10] = [
    "<p", "<li", "<td", "<th", "<h1", "<h2", "<h3", "<h4", "<h5", "<br",
];

/// `html` without emphasis delimiters left with nothing to emphasise: a delimiter opening a
/// text (at the start, or after a tag of [`BLOCK_OPENERS`]) followed by a space, and never closed in that run
/// ("<p>** These races ...</p>", what a transcription's "*(Paraphrased)*" becomes when only
/// the marker is taken out). `None` when there is none.
fn without_orphan_emphasis(html: &str) -> Option<String> {
    let mut out = String::with_capacity(html.len());
    let (mut rest, mut changed, mut at_start) = (html, false, true);
    loop {
        let run_end = rest.find('<').unwrap_or(rest.len());
        let run = &rest[..run_end];
        let orphan = ORPHAN_DELIMITERS.iter().find(|d| {
            at_start
                && run
                    .strip_prefix(**d)
                    .is_some_and(|r| r.starts_with(' ') && !r.contains(**d))
        });
        match orphan {
            Some(d) => {
                out.push_str(run[d.len()..].trim_start());
                changed = true;
            }
            None => out.push_str(run),
        }
        rest = &rest[run_end..];
        let Some(tag_end) = rest.find('>') else {
            out.push_str(rest);
            break;
        };
        let tag = &rest[..=tag_end];
        at_start = BLOCK_OPENERS.iter().any(|o| {
            tag.strip_prefix(o)
                .is_some_and(|r| r.starts_with(['>', ' ', '/']))
        });
        out.push_str(tag);
        rest = &rest[tag_end + 1..];
    }
    changed.then_some(out)
}

/// Emphasised transcription markers leave an element's description whole (see
/// [`without_wrapped_markers`]), and no emphasis a marker strip emptied is left (see
/// [`without_orphan_emphasis`]).
fn strip_wrapped_markers(e: &mut DraftElement) {
    let Some(d) = e.json.get("description").and_then(Value::as_str) else {
        return;
    };
    let markers = without_wrapped_markers(d);
    let base = markers.as_deref().unwrap_or(d);
    let orphans = without_orphan_emphasis(base);
    let Some(clean) = orphans.or(markers) else {
        return;
    };
    e.json["description"] = json!(clean);
    e.normalised.push(
        "description: transcription markers, and the emphasis they leave empty, left out".into(),
    );
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

/// Words of a credits line printed with the legal notice ("Writing and design: ...").
const CREDIT_WORDS: [&str; 12] = [
    "credits",
    "written by",
    "writing",
    "art by",
    "artwork",
    "illustrat",
    "cover art",
    "layout",
    "editing",
    "designed by",
    "cartograph",
    "playtest",
];

/// A sentence of a credits line.
fn credit(sentence: &str) -> bool {
    let low = plain(sentence).to_lowercase();
    CREDIT_WORDS.iter().any(|w| low.contains(w))
}

/// Words of an imprint line printed with the legal notice: who publishes, makes or
/// distributes the book, a postal address, a way to contact them.
const IMPRINT_WORDS: [&str; 14] = [
    "published by",
    "publisher:",
    "manufactured by",
    "represented by",
    "distributed by",
    "printed in ",
    "printed by",
    "po box",
    "p.o. box",
    "p. o. box",
    "contact us",
    "e-mail:",
    "email:",
    "registered office",
];

/// A sentence of an imprint line (publisher, manufacturer, address or contact): end matter,
/// like the notice it is printed with.
fn imprint(sentence: &str) -> bool {
    let low = plain(sentence).to_lowercase();
    IMPRINT_WORDS.iter().any(|w| low.contains(w))
}

/// End matter printed with a legal notice: a credits or imprint sentence.
fn notice_matter(sentence: &str) -> bool {
    credit(sentence) || imprint(sentence)
}

/// `before` ends in an initial ("by A" of "by A. Penn"): a stop after it ends no sentence.
fn initial(before: &str) -> bool {
    let word = before.rsplit([' ', '>']).next().unwrap_or("");
    word.chars().count() == 1 && word.chars().all(char::is_uppercase)
}

/// The sentences of a block's inner html (cut after ". ", "! " or "? " outside any tag and
/// any open inline element), each with its trailing space.
fn sentences(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut in_tag, mut from) = (0i32, false, 0usize);
    let bytes = inner.as_bytes();
    for (k, c) in inner.char_indices() {
        match c {
            '<' => {
                in_tag = true;
                let closing = bytes.get(k + 1) == Some(&b'/');
                let void = inner[k..].starts_with("<br");
                if !void {
                    depth += if closing { -1 } else { 1 };
                }
            }
            '>' => in_tag = false,
            '.' | '!' | '?'
                if !in_tag
                    && depth <= 0
                    && bytes.get(k + 1) == Some(&b' ')
                    && !initial(&inner[from..k]) =>
            {
                out.push(&inner[from..=k + 1]);
                from = k + 2;
            }
            _ => {}
        }
    }
    if from < inner.len() {
        out.push(&inner[from..]);
    }
    out
}

/// A legal paragraph that goes on with other text after its notice ("... All rights reserved.
/// This book adds ..."): the text after the last legal sentence (`None` when there is none).
/// The notice, and the credits sentences printed with it, are left out.
fn after_notice(block: &str) -> Option<String> {
    let open_end = block.find('>')? + 1;
    let close = block.rfind("</")?;
    let (open, inner, end) = (&block[..open_end], &block[open_end..close], &block[close..]);
    let parts = sentences(inner);
    let last = parts.iter().rposition(|s| legal(s))?;
    let first = parts.iter().position(|s| legal(s))?;
    let before: Vec<&str> = parts[..first]
        .iter()
        .copied()
        .filter(|s| !notice_matter(s))
        .collect();
    let after: Vec<&str> = parts[last + 1..]
        .iter()
        .copied()
        .filter(|s| !notice_matter(s))
        .collect();
    let kept = [before, after].concat().concat();
    let kept = kept.trim();
    (!plain(kept).trim().is_empty()).then(|| format!("{open}{kept}{end}"))
}

/// A paragraph whose every sentence is an imprint line (see [`imprint`]).
fn imprint_only(block: &str) -> bool {
    let inner = plain(block);
    let parts = sentences(&inner);
    !parts.is_empty()
        && parts.iter().all(|s| s.trim().is_empty() || imprint(s))
        && parts.iter().any(|s| imprint(s))
}

/// The legal or trademark notice leaves an element's description, unless the element is that
/// notice (end matter by name, or nothing but the notice): a paragraph that is only the notice
/// goes whole; one that goes on with other text (the notice and an intro printed as one
/// paragraph) keeps that text. Credits and imprint sentences printed with the notice go with
/// it, and so does a paragraph of imprint only next to a notice paragraph.
fn strip_legal(e: &mut DraftElement) {
    if super::passage::end_matter(&e.name) {
        return;
    }
    let Some(d) = e.json.get("description").and_then(Value::as_str) else {
        return;
    };
    let Some(parts) = blocks(d) else { return };
    let notice = |k: usize| parts.get(k).is_some_and(|p| legal(p));
    let gone: Vec<bool> = (0..parts.len())
        .map(|k| {
            notice(k)
                || (imprint_only(parts[k])
                    && (notice(k + 1) || k.checked_sub(1).is_some_and(notice)))
        })
        .collect();
    let kept: Vec<String> = parts
        .iter()
        .zip(&gone)
        .filter_map(|(p, &gone)| {
            if !gone {
                return Some((*p).to_string());
            }
            legal(p).then(|| after_notice(p)).flatten()
        })
        .collect();
    let dropped = gone.iter().filter(|g| **g).count();
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
