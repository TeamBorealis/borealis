//! An element's own printed passage, and what is built from it without the model: the
//! description of an element whose generated text only rewords its passage, the unplanned prose
//! sub-sections of class and subclass roots, the multiclassing section a class root needs, and
//! the report entries for source prose no element carries.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::fidelity::{pipeline_paragraph, plain, words, Index, MIN_COVERAGE};
use super::types::{ImportDraft, ImportPlan, PlannedElement, Role};
use crate::import::report::ReportEntry;

#[path = "passage_lore.rs"]
mod lore;
#[path = "page_marks.rs"]
pub(crate) mod page_marks;
#[path = "passage_tables.rs"]
pub(crate) mod tables;

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
/// Fewest words of a shorter paragraph it still looks at: a sentence that is the whole body of
/// its heading.
const MIN_SHORT_WORDS: usize = 5;
/// Most letters of a run-in label too short to name a report entry ("No." answering a question).
const MIN_LABEL_CHARS: usize = 3;
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

/// One headed sub-section: its title and its lines (heading left out), and where it is printed
/// (0-based heading line, exclusive end).
pub(crate) struct SubSection {
    pub title: String,
    pub lines: Vec<String>,
    pub at: usize,
    pub end: usize,
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

/// A text-only element starts at heading `line_idx` (or is named `title`).
fn planned_text(plan: &ImportPlan, line_idx: usize, title: &str) -> bool {
    let t = norm(title);
    plan.elements
        .iter()
        .filter(|e| e.role == Role::Text)
        .any(|e| {
            e.excerpt
                .as_ref()
                .is_some_and(|x| x.start_line.saturating_sub(1) == line_idx)
                || (!t.is_empty() && norm(&e.name) == t)
        })
}

/// The headed prose sub-sections inside a class or subclass root's section that no element is
/// planned for (a planned heading's own sub-headings left to it), each up to the next heading;
/// with flat headings, also the root-level headings before its first feature (a planned lore
/// section among them left out).
/// A class's hit point, proficiency and multiclassing headings are left to its rules.
pub(crate) fn unplanned_sections(
    source: &str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<SubSection> {
    let lines: Vec<&str> = source.lines().collect();
    unplanned_sections_in(&lines, planned, plan)
}

/// [`unplanned_sections`] of the source `lines`. A section another class or subclass root
/// that starts later prints as its own (a subclass's lore inside its class's section) is that
/// innermost root's, not this one's.
pub(crate) fn unplanned_sections_in(
    lines: &[&str],
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<SubSection> {
    let mut out = own_sections(lines, planned, plan);
    let Some(start) = planned.excerpt.as_ref().map(|x| x.start_line) else {
        return out;
    };
    let inner: Vec<usize> = plan
        .elements
        .iter()
        .filter(|e| e.id != planned.id && owns_sub_sections(e, &plan.elements))
        .filter(|e| e.excerpt.as_ref().is_some_and(|x| x.start_line > start))
        .flat_map(|e| own_sections(lines, e, plan))
        .map(|s| s.at)
        .collect();
    out.retain(|s| !inner.contains(&s.at));
    out
}

/// A root that prints the unplanned sub-sections of its section as its own text: a class or
/// subclass, or a standalone element with granted parts (a species whose traits are planned
/// under it, with its lore sections beside them).
pub(crate) fn owns_sub_sections(planned: &PlannedElement, elements: &[PlannedElement]) -> bool {
    match planned.role {
        Role::Class | Role::Subclass => true,
        Role::Standalone => elements.iter().any(|e| {
            [&e.owner, &e.parent]
                .iter()
                .any(|o| o.as_deref() == Some(planned.id.as_str()))
        }),
        _ => false,
    }
}

/// A line with no letter or digit (a stray glyph such as a lone "!" or bullet).
fn stray_glyph(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.chars().any(char::is_alphanumeric)
}

/// Heading words of a document's end matter (legal notice, credits, thanks), never a part of
/// the element printed before it.
const END_MATTER: [&str; 10] = [
    "legal",
    "license",
    "licence",
    "credits",
    "thanks",
    "acknowledgements",
    "acknowledgments",
    "copyright",
    "afterword",
    "colophon",
];

/// A heading title of end matter ("Legal Information", "Special Thanks To").
pub(crate) fn end_matter(title: &str) -> bool {
    words(title)
        .iter()
        .any(|w| END_MATTER.contains(&w.as_str()))
}

/// The first line (0-based) of the end matter after the last planned passage: the first
/// end-matter heading after it (an unmarked title line in a text document), or the first heading of that level or higher between them (an
/// unplanned closing section printed right before the legal notice is no part of the last
/// element either). `None` without end matter there.
pub(crate) fn end_matter_start(lines: &[&str], plan: &ImportPlan) -> Option<usize> {
    let at_end_matter = |l: usize| lines.get(l).is_some() && end_matter_title(lines, l);
    let last = starts(plan, "")
        .into_iter()
        .filter(|l| !at_end_matter(*l))
        .max()?;
    let e = (last + 1..lines.len()).find(|&k| at_end_matter(k))?;
    // A source without heading marks (a text document) has no levels to compare.
    let Some(level) = heading(lines[e]) else {
        return Some(e);
    };
    (last + 1..e)
        .find(|&k| heading(lines[k]).is_some_and(|h| h <= level))
        .or(Some(e))
}

/// Most words of an unmarked title line.
const TITLE_WORDS: usize = 6;

/// The line at `k` titles end matter: a heading with an end-matter title, or, in a source
/// without heading marks, a short line of its own (blank lines around it, no closing
/// punctuation) with one ("Legal Information" alone on its line).
pub(crate) fn end_matter_title(lines: &[&str], k: usize) -> bool {
    let t = lines[k].trim();
    if heading(t).is_some() {
        return end_matter(&heading_title(t));
    }
    let blank = |i: usize| lines.get(i).is_none_or(|l| l.trim().is_empty());
    let alone = (k == 0 || blank(k - 1)) && blank(k + 1);
    alone
        && !t.is_empty()
        && t.split_whitespace().count() <= TITLE_WORDS
        && !t.ends_with(['.', ',', ';', ':', '!', '?'])
        && end_matter(t)
}

/// The unplanned sub-sections in one root's section (see [`unplanned_sections_in`]). Page
/// furniture (running headers, page numbers) and stray glyph lines are left out of their text.
fn own_sections(lines: &[&str], planned: &PlannedElement, plan: &ImportPlan) -> Vec<SubSection> {
    let Some(ex) = planned.excerpt.as_ref() else {
        return Vec::new();
    };
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
    let mut furniture: Option<page_marks::PageFurniture> = None;
    let mut group: Option<(usize, SubSection)> = None;
    // The document's end matter is no sub-section of the last root.
    let stop = end_matter_start(lines, plan)
        .filter(|e| *e > start)
        .unwrap_or(lines.len());
    let mut i = start + 1;
    while i < stop {
        let Some(h) = heading(lines[i]) else {
            i += 1;
            continue;
        };
        let flat = h == depth && i < first_owned;
        if h < depth || (h == depth && !flat) {
            break;
        }
        let title = heading_title(lines[i]);
        // A rules or lore section planned on its own inside the root's flat intro does not end
        // the intro (its text is its own); any other planned heading there does.
        let text_here = flat && planned_text(plan, i, &title);
        if flat && !text_here && planned_heading(plan, &planned.id, i, &title) {
            break;
        }
        if skip_below.is_some_and(|s| h > s) {
            i += 1;
            continue;
        }
        skip_below = None;
        let low = title.to_lowercase();
        let block = planned.role == Role::Class && CLASS_BLOCK.iter().any(|b| low.contains(b));
        // A planned lore section in the flat intro carries its own text: the intro goes on.
        if planned_heading(plan, &planned.id, i, &title) || block {
            skip_below = Some(h);
            i += 1;
            continue;
        }
        let end = (i + 1..stop.min(i + MAX_LINES))
            .find(|&k| heading(lines[k]).is_some())
            .unwrap_or_else(|| stop.min(i + MAX_LINES));
        let furniture =
            furniture.get_or_insert_with(|| page_marks::page_furniture(&lines.join("\n")));
        let body: Vec<String> = (i + 1..end)
            .filter(|&k| !furniture.has(k) && !stray_glyph(lines[k]))
            .map(|k| lines[k].to_string())
            .collect();
        let prose = body.iter().any(|l| {
            !l.trim().is_empty() && !l.trim_start().starts_with('|') && !l.contains(" | ")
        });
        if prose {
            // A heading over the sub-sections, with no prose of its own, is printed above them.
            if let Some((level, group)) = group.take() {
                if h > level {
                    out.push(group);
                }
            }
            out.push(SubSection {
                title,
                lines: body,
                at: i,
                end,
            });
        } else {
            group = Some((
                h,
                SubSection {
                    title,
                    lines: Vec::new(),
                    at: i,
                    end,
                },
            ));
        }
        i = end;
    }
    out
}

/// Most words of a run-in label ("Heat Lance." opening a paragraph).
const RUN_IN_LABEL_WORDS: usize = 5;

/// A name without a trailing parenthetical ("Lamp Oil (20)" -> "lamp oil"), singular.
fn row_key(name: &str) -> String {
    let base = name
        .trim()
        .strip_suffix(')')
        .and_then(|r| r.rsplit_once(" ("))
        .map_or(name, |(b, _)| b);
    let n = norm(base);
    n.strip_suffix('s').map_or(n.clone(), str::to_string)
}

/// The label of a run-in paragraph line ("Heat Lance. When you ..." or "**Heat Lance.** ...").
fn run_in_label(line: &str) -> Option<String> {
    let t = line.trim_start().trim_start_matches("**");
    let (label, rest) = t.split_once(". ").or_else(|| t.split_once(".** "))?;
    let label = label.trim_end_matches("**");
    let words = label.split_whitespace().count();
    let opens = rest
        .trim_start()
        .trim_start_matches("**")
        .starts_with(char::is_uppercase);
    ((1..=RUN_IN_LABEL_WORDS).contains(&words) && label.starts_with(char::is_uppercase) && opens)
        .then(|| label.to_string())
}

/// A line after which a run-in label opens a new paragraph: blank, a heading, or a finished
/// sentence (a run-in printed right under the paragraph before it).
fn opens_paragraph(prev: &str) -> bool {
    prev.trim().is_empty() || heading(prev).is_some() || ends_sentence(prev)
}

/// Closing marks a finished sentence may end in ("... they rarely stray.]", "... (see below).)").
const CLOSING_MARKS: [char; 7] = [']', ')', '"', '\'', '\u{201d}', '\u{2019}', '*'];

/// `line` ends a sentence (a stop, perhaps inside closing brackets or quotes) or a label.
pub(crate) fn ends_sentence(line: &str) -> bool {
    line.trim_end()
        .trim_end_matches(CLOSING_MARKS)
        .ends_with(['.', '!', '?', ':'])
}

/// The keys a table row element answers to: its name, and the group its name opens before a
/// comma ("Lantern, hooded" -> "lantern").
fn row_keys(name: &str) -> Vec<String> {
    let mut keys = vec![row_key(name)];
    if let Some((head, _)) = name.split_once(", ") {
        keys.push(row_key(head));
    }
    keys
}

/// The row keys a run-in label names: one name, or several merged ("Lamp Oil and Smoke Oil",
/// "Wicks, Lamps, and Oils") when every one of them is a planned table row element.
fn label_keys(label: &str, plan: &ImportPlan) -> Vec<String> {
    let parts: Vec<String> = label
        .split(", ")
        .flat_map(|p| p.split(" and "))
        .map(|p| p.trim().trim_start_matches("and ").trim())
        .filter(|p| !p.is_empty())
        .map(row_key)
        .collect();
    if parts.len() < 2 {
        return vec![row_key(label)];
    }
    let rows: Vec<String> = plan
        .elements
        .iter()
        .filter(|e| e.role == Role::Standalone)
        .flat_map(|e| row_keys(&e.name))
        .collect();
    if parts.iter().all(|k| rows.contains(k)) {
        parts
    } else {
        vec![row_key(label)]
    }
}

/// The paragraph (label left out) of the one run-in in `source` labelled like the table row
/// element `planned` (its name without a count, singular or plural, or the group its name opens:
/// "Lanterns." for "Lantern, hooded"; a label may merge several rows' names): a row has no
/// prose of its own, its rule is printed after the table. `None` unless exactly one such
/// paragraph exists and no other element starts at it.
fn row_run_in(source: &str, planned: &PlannedElement, plan: &ImportPlan) -> Option<Vec<String>> {
    let lines: Vec<&str> = source.lines().collect();
    let row = planned.excerpt.as_ref()?.start_line.checked_sub(1)?;
    if !lines.get(row).is_some_and(|l| l.contains(" | ")) {
        return None;
    }
    let keys = row_keys(&planned.name);
    let mut hits = (0..lines.len()).filter(|&i| {
        heading(lines[i]).is_none()
            && (i == 0 || opens_paragraph(lines[i - 1]))
            && run_in_label(lines[i])
                .is_some_and(|l| label_keys(&l, plan).iter().any(|k| keys.contains(k)))
    });
    let at = hits.next()?;
    if hits.next().is_some() || starts(plan, &planned.id).contains(&at) {
        return None;
    }
    let end = (at + 1..lines.len())
        .find(|&k| lines[k].trim().is_empty() || heading(lines[k]).is_some() || run_in(lines[k]))
        .unwrap_or(lines.len());
    let label = run_in_label(lines[at]).unwrap_or_default();
    let mut out: Vec<String> = lines[at..end].iter().map(|l| l.to_string()).collect();
    out[0] = strip_label(&out[0], &label);
    Some(out)
}

/// Table row elements get the run-in paragraph the source prints for them after the table
/// (see [`row_run_in`]) unless their description already has it. Returns the ids changed.
fn attach_run_ins(draft: &mut ImportDraft) -> Vec<String> {
    let mut changed = Vec::new();
    for i in 0..draft.elements.len() {
        let e = &draft.elements[i];
        if e.json.is_null() || e.revisions > 0 || e.role != Role::Standalone {
            continue;
        }
        let Some(planned) = draft.plan.elements.iter().find(|p| p.id == e.id) else {
            continue;
        };
        let Some(para) = row_run_in(&draft.source_text, planned, &draft.plan) else {
            continue;
        };
        let desc = e
            .json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if share(&content(&para.join("\n")), &content(&plain(&desc))) >= SAME_CONTENT {
            continue;
        }
        let e = &mut draft.elements[i];
        e.json["description"] = json!(format!("{}{desc}", html_of(&para)));
        e.normalised
            .push("description: the rule the source prints for this row after its table".into());
        changed.push(e.id.clone());
    }
    changed
}

/// The printed title of a part's passage start: its heading, or its run-in label, or its name.
fn part_title(source: &str, part: &PlannedElement) -> String {
    let line = part
        .excerpt
        .as_ref()
        .and_then(|x| source.lines().nth(x.start_line.saturating_sub(1)))
        .unwrap_or("");
    if heading(line).is_some() {
        return heading_title(line);
    }
    let t = line.trim_start();
    t.strip_prefix("**")
        .and_then(|r| r.split_once("**"))
        .map(|(l, _)| l.trim().trim_end_matches(['.', ':']).to_string())
        .unwrap_or_else(|| part.name.clone())
}

/// Element types whose parts' printed text belongs in their own description.
const FEATURE_TYPES: [&str; 2] = ["Class Feature", "Archetype Feature"];

/// A planned feature with parts keeps their printed text in its own description, the way the
/// source nests it (the bundled Spellcasting features print Cantrips, Spell Slots and the rest
/// inside the feature): each part's passage the description lacks is appended in source order,
/// as a bold title and its text. (A species or background keeps its traits' text by its
/// brief; their rules are the traits' own.) Returns the ids changed.
fn add_part_text(draft: &mut ImportDraft) -> Vec<String> {
    let mut changed = Vec::new();
    for i in 0..draft.elements.len() {
        let e = &draft.elements[i];
        if e.json.is_null() || e.revisions > 0 || !FEATURE_TYPES.contains(&e.ty.as_str()) {
            continue;
        }
        let mut parts: Vec<&PlannedElement> = draft
            .plan
            .elements
            .iter()
            .filter(|c| c.role == Role::Part && c.parent.as_deref() == Some(e.id.as_str()))
            .filter(|c| c.excerpt.is_some())
            .collect();
        parts.sort_by_key(|c| c.excerpt.as_ref().map_or(0, |x| x.start_line));
        let mut desc = e
            .json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut added = Vec::new();
        for part in parts {
            let Some(passage) = own_passage(&draft.source_text, part, &draft.plan) else {
                continue;
            };
            let have = content(&plain(&desc));
            if share(&content(&passage.join("\n")), &have) >= SAME_CONTENT {
                continue;
            }
            let title = part_title(&draft.source_text, part);
            desc.push_str(&format!(
                "<p><strong>{}</strong></p>{}",
                title.replace('&', "&amp;").replace('<', "&lt;"),
                html_of(&passage)
            ));
            added.push(title);
        }
        if added.is_empty() {
            continue;
        }
        let e = &mut draft.elements[i];
        e.json["description"] = json!(desc);
        e.normalised.push(format!(
            "description: the printed text of its part(s) {} added, as the source nests them",
            added.join(", ")
        ));
        changed.push(e.id.clone());
    }
    changed
}

/// A table line ("a | b").
fn table_line(line: &str) -> bool {
    let t = line.trim();
    t.contains(" | ") || t.starts_with('|')
}

/// The blocks of a sub-section's `lines` (blank-line separated; a table apart from the prose
/// around it) whose content `have` (a description's content words) lacks, blank-line
/// separated, and whether some block was already there.
fn missing_blocks(lines: &[String], have: &HashSet<String>) -> (Vec<String>, bool) {
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut prev: Option<bool> = None;
    for l in lines {
        if l.trim().is_empty() {
            prev = None;
            continue;
        }
        let table = table_line(l);
        // A wrapped cell (no pipe, lower case) stays in its table.
        let wrapped = prev == Some(true) && l.trim_start().starts_with(char::is_lowercase);
        if prev.is_none() || (prev != Some(table) && !wrapped) {
            blocks.push(Vec::new());
        }
        blocks.last_mut().expect("a block").push(l.clone());
        prev = Some(table || wrapped);
    }
    let mut out: Vec<String> = Vec::new();
    let mut partial = false;
    for b in blocks {
        if share(&content(&b.join("\n")), have) >= SAME_CONTENT {
            partial = true;
            continue;
        }
        if !out.is_empty() {
            out.push(String::new());
        }
        out.extend(b);
    }
    (out, partial)
}

/// The unplanned sub-sections of a class or subclass root appended to its description (as a
/// bold heading and its printed text) unless the description already has their content; table
/// rows get the run-in paragraph printed for them (see [`attach_run_ins`]); an element with
/// parts gets their printed text (see [`add_part_text`]).
pub(crate) fn add_sub_sections(draft: &mut ImportDraft) -> Vec<String> {
    let mut changed = attach_run_ins(draft);
    for id in add_part_text(draft) {
        if !changed.contains(&id) {
            changed.push(id);
        }
    }
    for i in 0..draft.elements.len() {
        let e = &draft.elements[i];
        if e.json.is_null() || e.revisions > 0 {
            continue;
        }
        let Some(planned) = draft
            .plan
            .elements
            .iter()
            .find(|p| p.id == e.id && owns_sub_sections(p, &draft.plan.elements))
        else {
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
        // A heading over sub-sections is printed only above one that is added.
        let mut group: Option<String> = None;
        for s in subs {
            let have = content(&plain(&desc));
            if s.lines.is_empty() {
                let printed = plain(&desc)
                    .to_lowercase()
                    .contains(&s.title.to_lowercase());
                group = (!printed).then_some(s.title);
                continue;
            }
            if share(&content(&s.lines.join("\n")), &have) >= SAME_CONTENT {
                continue;
            }
            // Only the blocks the description lacks; the heading too unless the description
            // already prints it over the blocks it has.
            let (lines, partial) = missing_blocks(&s.lines, &have);
            if lines.is_empty() {
                continue;
            }
            let titled = plain(&desc)
                .to_lowercase()
                .contains(&s.title.to_lowercase());
            if let Some(title) = group.take() {
                desc.push_str(&format!(
                    "<p><strong>{}</strong></p>",
                    title.replace('&', "&amp;").replace('<', "&lt;")
                ));
            }
            if !(partial && titled) {
                desc.push_str(&format!(
                    "<p><strong>{}</strong></p>",
                    s.title.replace('&', "&amp;").replace('<', "&lt;")
                ));
            }
            desc.push_str(&html_of(&lines));
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
    for id in tables::add_owner_tables(draft) {
        if !changed.contains(&id) {
            changed.push(id);
        }
    }
    for id in lore::restore_opening_lore(draft) {
        if !changed.contains(&id) {
            changed.push(id);
        }
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

/// Source paragraphs (blank-line or heading separated, page furniture left out) with the line
/// of the heading they are under.
fn paragraphs(source: &str) -> Vec<(usize, Option<usize>, String)> {
    let furniture = page_marks::page_furniture(source);
    let mut out = Vec::new();
    let mut under: Option<usize> = None;
    let mut cur: Vec<&str> = Vec::new();
    let mut first = 0;
    let lines: Vec<&str> = source.lines().collect();
    let mut flush = |cur: &mut Vec<&str>, under: Option<usize>, first: usize| {
        if !cur.is_empty() {
            out.push((first, under, cur.join("\n")));
            cur.clear();
        }
    };
    for (i, l) in lines.iter().enumerate() {
        // Page furniture is no prose; it ends a paragraph like a blank line.
        if heading(l).is_some() || l.trim().is_empty() || l.contains(" | ") || furniture.has(i) {
            flush(&mut cur, under, first);
            if heading(l).is_some() {
                under = Some(i);
            }
            continue;
        }
        // A run-in label after a finished sentence opens a paragraph of its own ("Starting
        // Coin." printed right under the paragraph before it).
        let opens = cur.last().is_some_and(|p| opens_paragraph(p))
            && (run_in(l) || run_in_label(l).is_some());
        if opens {
            flush(&mut cur, under, first);
        }
        if cur.is_empty() {
            first = i;
        }
        cur.push(l);
    }
    flush(&mut cur, under, first);
    out
}

/// The heading an uncovered paragraph is reported under: the widest section around it that no
/// planned element starts in ("Printing Notes" for prose under its unplanned sub-headings),
/// else the heading right above it.
fn report_heading(lines: &[&str], at: usize, planned: &[usize]) -> String {
    let section_end = |k: usize, level: usize| {
        (k + 1..lines.len())
            .find(|&j| heading(lines[j]).is_some_and(|h| h <= level))
            .unwrap_or(lines.len())
    };
    let free = |k: usize, level: usize| {
        let end = section_end(k, level);
        !planned.iter().any(|p| *p >= k && *p < end)
    };
    let mut best = at;
    let mut k = at;
    while let Some(level) = heading(lines[k]) {
        if !free(k, level) {
            break;
        }
        best = k;
        let Some(up) = (0..k)
            .rev()
            .find(|&j| heading(lines[j]).is_some_and(|h| h < level))
        else {
            break;
        };
        k = up;
    }
    heading_title(lines[best])
}

/// A short paragraph that is still real prose: a sentence of [`MIN_SHORT_WORDS`] words or more
/// (a capital first, a full stop last), not a label or a list line.
fn short_prose(text: &str, words: usize) -> bool {
    let t = text.trim();
    words >= MIN_SHORT_WORDS
        && t.starts_with(|c: char| c.is_uppercase())
        && t.ends_with(['.', '!', '?'])
        && !t
            .lines()
            .any(|l| l.trim_start().starts_with(['-', '*', '•']))
}

/// Report entries for source prose no element carries: a paragraph outside every planned
/// passage whose 4-grams are mostly in no description (a short one when it is a sentence and
/// the whole body of its heading). Consecutive ones under one heading are
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
    // The ranges each element's description was built from: a text-only element's section
    // blocks, a generated element's located passage up to the next planned passage (a child
    // or sibling planned inside it carries that text itself).
    let src_lines: Vec<&str> = draft.source_text.lines().collect();
    let tail = end_matter_start(&src_lines, &draft.plan).unwrap_or(usize::MAX);
    let ranges: Vec<(usize, usize)> = draft
        .plan
        .elements
        .iter()
        .flat_map(|p| {
            let Some(ex) = p.excerpt.as_ref() else {
                return Vec::new();
            };
            if p.role == Role::Text {
                return super::parts::section_blocks(&draft.source_text, p, &draft.plan);
            }
            let s = ex.start_line.saturating_sub(1);
            let next = starts(&draft.plan, &p.id)
                .into_iter()
                .filter(|l| *l > s)
                .min()
                .unwrap_or(usize::MAX);
            // A generated element's passage never covers the end matter after it.
            let end = if s < tail { next.min(tail) } else { next };
            vec![(s, ex.end_line.min(end).max(s + 1))]
        })
        .collect();
    let mut out: Vec<ReportEntry> = Vec::new();
    let lines: Vec<&str> = draft.source_text.lines().collect();
    let planned = starts(&draft.plan, "");
    let paras = paragraphs(&draft.source_text);
    // A paragraph printed more than once (each file's cover or legal block in a multi-file
    // source) is covered only as often as the descriptions print it.
    let key = |w: &[String]| w.join(" ");
    let mut printed: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (_, _, text) in &paras {
        *printed.entry(key(&words(text))).or_default() += 1;
    }
    let described = format!(" {} ", words(&all).join(" "));
    for (line, under, text) in &paras {
        let (line, under) = (*line, *under);
        let w = words(text);
        let alone = under.is_some() && paras.iter().filter(|p| p.1 == under).count() == 1;
        let short = w.len() < MIN_PARAGRAPH_WORDS && !(alone && short_prose(text, w.len()));
        if short || ranges.iter().any(|(s, e)| line >= *s && line < *e) {
            continue;
        }
        let times = printed.get(&key(&w)).copied().unwrap_or(1);
        let held = times <= 1 || described.matches(&format!(" {} ", key(&w))).count() >= times;
        if held && super::fidelity::coverage(&w, &index, &none).is_some_and(|c| c >= MIN_COVERAGE) {
            continue;
        }
        let first: Vec<&str> = text.split_whitespace().take(QUOTED_WORDS).collect();
        // A run-in paragraph is reported under its label ("Starting Coin."), any other
        // under its heading.
        let label = text
            .lines()
            .next()
            .and_then(run_in_label)
            .filter(|l| !l.contains(',') && l.chars().count() > MIN_LABEL_CHARS);
        let source = label.unwrap_or_else(|| {
            under.map_or_else(
                // A source without heading marks: under its end-matter title line, if any.
                || {
                    (0..line)
                        .rev()
                        .find(|&k| end_matter_title(&lines, k))
                        .map_or_else(
                            || "source text before the first heading".to_string(),
                            |k| lines[k].trim().to_string(),
                        )
                },
                |h| report_heading(&lines, h, &planned),
            )
        });
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
#[path = "passage_bounds_tests.rs"]
mod bounds_tests;
#[cfg(test)]
#[path = "passage_tests.rs"]
mod tests;
