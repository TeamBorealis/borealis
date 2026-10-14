//! An element's own printed passage, and what is built from it without the model: the
//! description of an element whose generated text only rewords its passage, the unplanned prose
//! sub-sections of class and subclass roots, the multiclassing section a class root needs, and
//! the report entries for source prose no element carries.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::fidelity::{pipeline_paragraph, plain, words, Index, MIN_COVERAGE};
use super::types::{ImportDraft, ImportPlan, PlannedElement, Role};
use crate::import::report::ReportEntry;

/// Most lines of one passage or sub-section.
const MAX_LINES: usize = 400;
/// Share of content words a description and a passage must have in common both ways to be the
/// same text (a rewording of the passage, nothing added or dropped).
const SAME_CONTENT: f64 = 0.75;
/// A passage already kept as printed (share of its description's 4-grams in the passage).
const VERBATIM: f64 = 0.9;
/// Most lines of a multiclassing section.
const MULTICLASS_LINES: usize = 60;
/// Fewest words of a source paragraph the uncovered-prose check looks at.
const MIN_PARAGRAPH_WORDS: usize = 12;
/// Most paragraphs quoted per uncovered-prose entry (by their first words).
const QUOTED_WORDS: usize = 8;
/// Headings of a class's hit point and proficiency block (setters and rules carry them).
const CLASS_BLOCK: [&str; 6] = [
    "hit points",
    "hit dice",
    "proficiencies",
    "equipment",
    "class features",
    "multiclass",
];
/// Words never counted when comparing content.
const STOP: [&str; 30] = [
    "a", "an", "the", "of", "to", "and", "or", "in", "on", "at", "by", "for", "with", "you",
    "your", "is", "are", "be", "can", "as", "it", "its", "this", "that", "if", "when", "than",
    "then", "from", "into",
];

fn heading(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    (n > 0 && t[n..].starts_with(' ')).then_some(n)
}

fn heading_title(line: &str) -> String {
    let t = line.trim_start();
    t.trim_start_matches('#')
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_string()
}

/// A line opening with a bold run-in label ("**Name.** text").
fn run_in(line: &str) -> bool {
    let t = line.trim_start();
    t.strip_prefix("**")
        .and_then(|r| r.split_once("**"))
        .is_some_and(|(label, _)| label.len() <= 60 && label.trim_end().ends_with(['.', ':']))
}

/// `line` without its leading run-in label (bold, or the element's own name).
fn strip_label(line: &str, name: &str) -> String {
    let t = line.trim_start();
    if run_in(t) {
        let rest = &t[2..];
        if let Some((_, after)) = rest.split_once("**") {
            return after.trim_start().to_string();
        }
    }
    let named = t
        .get(..name.len())
        .is_some_and(|head| !name.is_empty() && head.eq_ignore_ascii_case(name));
    match t.get(name.len()..) {
        Some(after) if named && after.starts_with(['.', ':']) => {
            after[1..].trim_start().to_string()
        }
        _ => t.to_string(),
    }
}

/// A spell's stat line ("Casting Time: 1 action", "2nd-level necromancy", "Evocation cantrip").
fn stat_line(line: &str) -> bool {
    let t = line.trim();
    let n = t.split_whitespace().count();
    let label = t
        .split_once(':')
        .is_some_and(|(l, _)| !l.is_empty() && l.split_whitespace().count() <= 3);
    let low = t.to_lowercase();
    n <= 8
        && (label || low.contains("cantrip") || low.contains("-level") || low.starts_with("level "))
}

fn starts(plan: &ImportPlan, except: &str) -> Vec<usize> {
    plan.elements
        .iter()
        .filter(|e| e.id != except)
        .filter_map(|e| e.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1)))
        .collect()
}

/// The element's own printed passage: from its heading (left out) to the next heading of the
/// same or a higher level, or from its run-in label (left out) to the next label or heading;
/// never past the next planned passage. `None` without a passage of its own: unlocated, a
/// passage another element starts at too, or a table row.
pub(crate) fn own_passage(
    source: &str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Option<Vec<String>> {
    let ex = planned.excerpt.as_ref()?;
    let lines: Vec<&str> = source.lines().collect();
    let start = ex.start_line.checked_sub(1).filter(|s| *s < lines.len())?;
    let others = starts(plan, &planned.id);
    if others.contains(&start) {
        return None;
    }
    let next = others
        .iter()
        .copied()
        .filter(|l| *l > start)
        .min()
        .unwrap_or(lines.len());
    let own = heading(lines[start]);
    let mut end = start + 1;
    while end < lines.len().min(next).min(start + MAX_LINES) {
        let h = heading(lines[end]);
        let stop = match own {
            Some(o) => h.is_some_and(|h| h <= o),
            None => h.is_some() || run_in(lines[end]),
        };
        if stop {
            break;
        }
        end += 1;
    }
    let mut out: Vec<String> = lines[start..end].iter().map(|l| l.to_string()).collect();
    if own.is_some() {
        out.remove(0);
    } else {
        out[0] = strip_label(&out[0], &planned.name);
    }
    if planned.ty == "Spell" {
        let body = out
            .iter()
            .position(|l| !l.trim().is_empty() && !stat_line(l))
            .unwrap_or(out.len());
        out.drain(..body);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    while out.first().is_some_and(|l| l.trim().is_empty()) {
        out.remove(0);
    }
    let rows = out.iter().filter(|l| l.contains(" | ")).count();
    (!out.is_empty() && rows == 0).then_some(out)
}

/// Lines with a line-end hyphen joined to the next line ("de-" + "termining").
fn joined(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in lines {
        let t = l.trim_end();
        match out.last_mut() {
            Some(prev)
                if prev.ends_with('-')
                    && prev.chars().rev().nth(1).is_some_and(char::is_alphabetic)
                    && t.trim_start().starts_with(char::is_lowercase) =>
            {
                prev.pop();
                prev.push_str(t.trim_start());
            }
            _ => out.push(t.to_string()),
        }
    }
    out
}

/// Printed lines as description HTML.
pub(crate) fn html_of(lines: &[String]) -> String {
    let lines = joined(lines);
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    super::parts::to_html(&refs)
}

fn content(text: &str) -> HashSet<String> {
    words(text)
        .into_iter()
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect()
}

fn share(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.len() as f64
}

/// Roles whose description is replaced by the printed passage when it only rewords it.
fn restorable(role: Role) -> bool {
    matches!(
        role,
        Role::Part
            | Role::Option
            | Role::ClassFeature
            | Role::SubclassFeature
            | Role::Standalone
            | Role::Subclass
    )
}

/// The printed passage for `desc` when `desc` says the same as `passage` in other words (same
/// content words both ways, not already verbatim); the pipeline's own paragraphs are kept.
fn reworded(desc: &str, passage: &[String]) -> Option<String> {
    let (kept, own): (Vec<&str>, Vec<&str>) = desc
        .split_inclusive("</p>")
        .partition(|p| pipeline_paragraph(p));
    let own = own.concat();
    let html = html_of(passage);
    let (d, p) = (content(&plain(&own)), content(&plain(&html)));
    if share(&d, &p) < SAME_CONTENT || share(&p, &d) < SAME_CONTENT {
        return None;
    }
    let index = Index::new(&passage.join("\n"));
    let none = HashSet::new();
    let w = words(&plain(&own));
    if super::fidelity::coverage(&w, &index, &none).is_some_and(|c| c >= VERBATIM) {
        return None;
    }
    Some(format!("{html}{}", kept.concat()))
}

/// Descriptions that only reword their element's own passage become the passage as printed.
/// Elements the user revised, and elements with parts or options (their description keeps the
/// parts' text too), are left alone. Returns the ids changed.
pub(crate) fn restore(draft: &mut ImportDraft) -> Vec<String> {
    let mut changed = Vec::new();
    for i in 0..draft.elements.len() {
        let e = &draft.elements[i];
        if e.json.is_null() || e.revisions > 0 || !restorable(e.role) {
            continue;
        }
        let Some(planned) = draft.plan.elements.iter().find(|p| p.id == e.id) else {
            continue;
        };
        // An element with parts or options keeps their text too, past its own passage.
        let parent = draft
            .plan
            .elements
            .iter()
            .any(|c| c.parent.as_deref() == Some(e.id.as_str()));
        if parent {
            continue;
        }
        let Some(passage) = own_passage(&draft.source_text, planned, &draft.plan) else {
            continue;
        };
        let desc = e
            .json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(html) = reworded(desc, &passage) else {
            continue;
        };
        let e = &mut draft.elements[i];
        e.json["description"] = json!(html);
        let note = "description: the printed passage (the generated text reworded it)".to_string();
        if !e.normalised.contains(&note) {
            e.normalised.push(note);
        }
        changed.push(e.id.clone());
    }
    changed
}

/// A table row element (its passage starts on a table row) has no prose of its own: description
/// sentences the source does not have are dropped (the row's cells stay, as the pipeline's
/// column lines). Returns the ids changed.
pub(crate) fn drop_unprinted_row_text(draft: &mut ImportDraft) -> Vec<String> {
    let index = Index::new(&draft.source_text);
    let mask = super::fidelity::name_mask(draft);
    let lines: Vec<&str> = draft.source_text.lines().collect();
    let rows: Vec<String> = draft
        .plan
        .elements
        .iter()
        .filter(|p| p.role == Role::Standalone)
        .filter(|p| {
            p.excerpt
                .as_ref()
                .and_then(|x| lines.get(x.start_line.saturating_sub(1)))
                .is_some_and(|l| l.contains(" | "))
        })
        .map(|p| p.id.clone())
        .collect();
    let mut changed = Vec::new();
    for e in draft.elements.iter_mut() {
        if e.json.is_null() || e.revisions > 0 || !rows.contains(&e.id) {
            continue;
        }
        let desc = e
            .json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        let untraced = super::fidelity::untraced(&e.json, &index, &mask);
        if untraced.is_empty() {
            continue;
        }
        let kept: String = desc
            .split_inclusive("</p>")
            .filter(|p| {
                pipeline_paragraph(p)
                    || super::fidelity::sentences(p)
                        .iter()
                        .any(|s| !untraced.contains(s))
            })
            .collect();
        if kept == desc {
            continue;
        }
        e.json["description"] = json!(kept);
        e.normalised.push(format!(
            "description: {} sentence(s) the source does not print removed (a table row has no prose of its own)",
            untraced.len()
        ));
        changed.push(e.id.clone());
    }
    changed
}

/// One headed sub-section: its title and its lines (heading left out).
pub(crate) struct SubSection {
    pub title: String,
    pub lines: Vec<String>,
}

fn norm(s: &str) -> String {
    words(s).join(" ")
}

/// A heading that is a planned element: one starts at it, or it is a planned name, alone or
/// with a level after it ("Second Wind 2nd").
fn planned_heading(plan: &ImportPlan, root: &str, line_idx: usize, title: &str) -> bool {
    let t = norm(title);
    plan.elements.iter().filter(|e| e.id != root).any(|e| {
        e.excerpt
            .as_ref()
            .is_some_and(|x| x.start_line.saturating_sub(1) == line_idx)
            || {
                let n = norm(&e.name);
                let level = t
                    .strip_prefix(&format!("{n} "))
                    .and_then(|rest| rest.split(' ').next())
                    .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()));
                !n.is_empty() && (t == n || level)
            }
    })
}

/// The headed prose sub-sections inside a class or subclass root's section that no element is
/// planned for (a planned heading's own sub-headings included), each up to the next heading;
/// with flat headings, also the root-level headings before its first feature.
/// A class's hit point, proficiency and multiclassing headings are left to its rules.
pub(crate) fn unplanned_sections(
    source: &str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<SubSection> {
    let Some(ex) = planned.excerpt.as_ref() else {
        return Vec::new();
    };
    let lines: Vec<&str> = source.lines().collect();
    let start = ex.start_line.saturating_sub(1);
    let Some(depth) = lines.get(start).and_then(|l| heading(l)) else {
        return Vec::new();
    };
    // A source with flat headings prints the class's intro sections ("Creating a ...") at the
    // root's own level, before its first feature.
    let first_owned = plan
        .elements
        .iter()
        .filter(|e| e.owner.as_deref() == Some(planned.id.as_str()))
        .filter_map(|e| e.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1)))
        .filter(|l| *l > start)
        .min()
        .unwrap_or(start);
    let mut out = Vec::new();
    let mut skip_below: Option<usize> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let Some(h) = heading(lines[i]) else {
            i += 1;
            continue;
        };
        let flat = h == depth && i < first_owned;
        if h < depth || (h == depth && !flat) {
            break;
        }
        if flat && planned_heading(plan, &planned.id, i, &heading_title(lines[i])) {
            break;
        }
        if skip_below.is_some_and(|s| h > s) {
            i += 1;
            continue;
        }
        skip_below = None;
        let title = heading_title(lines[i]);
        let low = title.to_lowercase();
        let block = planned.role == Role::Class && CLASS_BLOCK.iter().any(|b| low.contains(b));
        if planned_heading(plan, &planned.id, i, &title) || block {
            skip_below = Some(h);
            i += 1;
            continue;
        }
        let end = (i + 1..lines.len().min(i + MAX_LINES))
            .find(|&k| heading(lines[k]).is_some())
            .unwrap_or_else(|| lines.len().min(i + MAX_LINES));
        let body: Vec<String> = lines[i + 1..end].iter().map(|l| l.to_string()).collect();
        let prose = body.iter().any(|l| {
            !l.trim().is_empty() && !l.trim_start().starts_with('|') && !l.contains(" | ")
        });
        if prose {
            out.push(SubSection { title, lines: body });
        }
        i = end;
    }
    out
}

/// The unplanned sub-sections of a class or subclass root appended to its description (as a
/// bold heading and its printed text) unless the description already has their content.
pub(crate) fn add_sub_sections(draft: &mut ImportDraft) -> Vec<String> {
    let mut changed = Vec::new();
    for i in 0..draft.elements.len() {
        let e = &draft.elements[i];
        if e.json.is_null() || e.revisions > 0 || !matches!(e.role, Role::Class | Role::Subclass) {
            continue;
        }
        let Some(planned) = draft.plan.elements.iter().find(|p| p.id == e.id) else {
            continue;
        };
        let subs = unplanned_sections(&draft.source_text, planned, &draft.plan);
        let mut desc = e
            .json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut added = Vec::new();
        for s in subs {
            let have = content(&plain(&desc));
            if share(&content(&s.lines.join("\n")), &have) >= SAME_CONTENT {
                continue;
            }
            desc.push_str(&format!(
                "<p><strong>{}</strong></p>{}",
                s.title.replace('&', "&amp;").replace('<', "&lt;"),
                html_of(&s.lines)
            ));
            added.push(s.title);
        }
        if added.is_empty() {
            continue;
        }
        let e = &mut draft.elements[i];
        e.json["description"] = json!(desc);
        e.normalised.push(format!(
            "description: printed sub-section(s) {} added (no element of their own)",
            added.join(", ")
        ));
        changed.push(e.id.clone());
    }
    changed
}

/// The multiclassing section nearest to a class root that lies outside its located passage (a
/// heading naming multiclassing, with its sub-headings), for the root's generation source.
pub(crate) fn multiclass_section(source: &str, planned: &PlannedElement) -> Option<String> {
    if planned.role != Role::Class {
        return None;
    }
    let ex = planned.excerpt.as_ref()?;
    let (s, e) = (ex.start_line.saturating_sub(1), ex.end_line);
    let lines: Vec<&str> = source.lines().collect();
    let at = (0..lines.len())
        .filter(|&i| i < s || i >= e)
        .filter(|&i| heading(lines[i]).is_some())
        .filter(|&i| lines[i].to_lowercase().contains("multiclass"))
        .min_by_key(|i| i.abs_diff(s))?;
    let depth = heading(lines[at])?;
    let end = (at + 1..lines.len().min(at + MULTICLASS_LINES))
        .find(|&k| heading(lines[k]).is_some_and(|h| h <= depth))
        .unwrap_or_else(|| lines.len().min(at + MULTICLASS_LINES));
    Some(lines[at..end].join("\n"))
}

/// Source paragraphs (blank-line or heading separated) with the heading they are under.
fn paragraphs(source: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    let mut under = String::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut first = 0;
    let lines: Vec<&str> = source.lines().collect();
    let mut flush = |cur: &mut Vec<&str>, under: &str, first: usize| {
        if !cur.is_empty() {
            out.push((first, under.to_string(), cur.join("\n")));
            cur.clear();
        }
    };
    for (i, l) in lines.iter().enumerate() {
        if heading(l).is_some() || l.trim().is_empty() || l.contains(" | ") {
            flush(&mut cur, &under, first);
            if heading(l).is_some() {
                under = heading_title(l);
            }
            continue;
        }
        if cur.is_empty() {
            first = i;
        }
        cur.push(l);
    }
    flush(&mut cur, &under, first);
    out
}

/// Report entries for source prose no element carries: a paragraph outside every planned
/// passage whose 4-grams are mostly in no description. Consecutive ones under one heading are
/// one entry. Not a failure: the user decides whether it matters.
pub(crate) fn uncovered_prose(draft: &ImportDraft) -> Vec<ReportEntry> {
    let all: String = draft
        .elements
        .iter()
        .filter_map(|e| e.json.get("description").and_then(Value::as_str))
        .map(plain)
        .collect::<Vec<_>>()
        .join("\n");
    let index = Index::new(&all);
    let none = HashSet::new();
    let ranges: Vec<(usize, usize)> = draft
        .plan
        .elements
        .iter()
        .filter_map(|p| {
            let ex = p.excerpt.as_ref()?;
            let n = super::parts::section_lines(&draft.source_text, p, &draft.plan).len();
            let s = ex.start_line.saturating_sub(1);
            Some((
                s,
                s + n.max(ex.end_line + 1 - ex.start_line.min(ex.end_line)),
            ))
        })
        .collect();
    let mut out: Vec<ReportEntry> = Vec::new();
    for (line, under, text) in paragraphs(&draft.source_text) {
        let w = words(&text);
        if w.len() < MIN_PARAGRAPH_WORDS || ranges.iter().any(|(s, e)| line >= *s && line < *e) {
            continue;
        }
        if super::fidelity::coverage(&w, &index, &none).is_some_and(|c| c >= MIN_COVERAGE) {
            continue;
        }
        let first: Vec<&str> = text.split_whitespace().take(QUOTED_WORDS).collect();
        let source = if under.is_empty() {
            "source text before the first heading".to_string()
        } else {
            under
        };
        match out.last_mut().filter(|e| e.source == source) {
            Some(e) => e.note.push_str(&format!("; \"{} ...\"", first.join(" "))),
            None => out.push(ReportEntry {
                source,
                outcome: "skipped".into(),
                elements: Vec::new(),
                note: format!(
                    "prose in no element's description (check it is not rules text): \"{} ...\"",
                    first.join(" ")
                ),
            }),
        }
    }
    out
}

#[cfg(test)]
#[path = "passage_tests.rs"]
mod tests;
