//! Acceptance checks derived from the source itself, with no model: every content heading of
//! the source is imported, every table row inside an element's passage is carried, every number
//! in an element's mechanics is printed in its passage, every reference resolves and the
//! description is the source's text. They generalise a hand-written list of expectations to any
//! homebrew. A failed check is a "check this" on its element (or on the import, for a heading);
//! [`heal`] gives the elements that fail one a single targeted repair call.
//!
//! Precision comes before recall: a check fails only on evidence that holds on any layout
//! (a number neither the passage nor the rules library prints, a row most of whose words the
//! element lacks), so a failed check is worth the user's look.
//!
//! Stable codes (the review state maps them): [`HEADING_UNCOVERED`] (import level),
//! [`TABLE_ROW_MISSING`], [`NUMBER_NOT_IN_SOURCE`], [`REFERENCE_UNRESOLVED`],
//! [`DESCRIPTION_UNTRACED`]. A passed check is recorded once per code the element was checked
//! for, so "checked and fine" differs from "not checked".

use std::collections::{HashMap, HashSet};

use borealis_ai::task::{AiRunner, RunOptions};
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::issue::IssueCode;
use borealis_content::{ContentStore, ContentStoreBuilder};
use serde_json::Value;

use super::fidelity::{coverage, name_mask, plain, untraced, words, Index};
use super::generate::{
    element_issues, emit, parent_of, refused, run_element_call, siblings_of, validate_rounds,
    GenCtx, Progress,
};
use super::types::{
    DraftElement, ElementCheck, ElementStatus, ImportDraft, PlannedElement, Role, Stage,
};
use crate::homebrew::PackContext;

#[path = "selfcheck_numbers.rs"]
mod numbers;
#[path = "selfcheck_rows.rs"]
mod rows;

/// A content heading of the source that no element carries (an import-level check).
pub(crate) const HEADING_UNCOVERED: &str = "heading_uncovered";
/// A data row of a table inside the element's passage that the element does not carry, or a
/// feature of a progression table with no element.
pub(crate) const TABLE_ROW_MISSING: &str = "table_row_missing";
/// A number of the element's mechanics that neither its source passage nor the library prints.
pub(crate) const NUMBER_NOT_IN_SOURCE: &str = "number_not_in_source";
/// A grant, choice or requirement pointing at an id neither the import nor the library has.
pub(crate) const REFERENCE_UNRESOLVED: &str = "reference_unresolved";
/// Description sentences the source does not have (the fidelity check's result).
pub(crate) const DESCRIPTION_UNTRACED: &str = "description_untraced";

/// Fewest words of a heading's own body that make it a content heading.
const MIN_BODY_WORDS: usize = 12;
/// Most words of a section introduction: a short body followed by deeper sub-headings is the
/// lead-in of its sub-sections, not content of its own.
const MAX_INTRO_WORDS: usize = 40;
/// Fewest words of a prose line (a wrapped sentence); a body most of whose lines are shorter is
/// a table's column, not a section.
const MIN_PROSE_LINE_WORDS: usize = 5;
/// Share of a section body's 4-grams found in the descriptions that counts it as imported.
const BODY_COVERED: f64 = 0.5;
/// Words of a heading or body that mark the publication's legal or credits pages.
const LEGAL_WORDS: [&str; 5] = ["copyright", "trademark", "license", "licence", "credits"];
/// Most missing things named in one message.
const MAX_NAMED: usize = 3;
/// The model step of a source-check repair (its own name, so call logs and replays tell it
/// from the sweep's repairs).
const CHECK_REPAIR_STEP: &str = "import_check_repair";
/// Most elements repaired by [`heal`] in one import (the worst first).
const MAX_HEAL_REPAIRS: usize = 16;

// ---------------------------------------------------------------------------------------------
// Text helpers (shared with the numbers and rows checks)

/// Lowercase alphanumeric tokens ("1d8", "2nd", "feet"); thousands commas dropped first.
fn tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut clean = String::with_capacity(text.len());
    for (i, c) in chars.iter().enumerate() {
        let thousands = *c == ','
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(char::is_ascii_digit)
            && chars.get(i + 4).map_or(true, |c| !c.is_ascii_digit());
        if !thousands {
            clean.extend(c.to_lowercase());
        }
    }
    clean
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// A dice token ("2d6" = (2, 6), "d8" = (1, 8)).
fn dice(token: &str) -> Option<(u32, u32)> {
    let (count, size) = token.split_once('d')?;
    let size: u32 = size.parse().ok()?;
    let count = if count.is_empty() {
        1
    } else {
        count.parse().ok()?
    };
    Some((count, size))
}

/// The leading number of a token ("15gp" = 15, "2nd" = 2), not of a dice token.
fn leading_number(token: &str) -> Option<u64> {
    let digits: String = token.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || dice(token).is_some() {
        return None;
    }
    digits.parse().ok()
}

/// Words joined lowercase, for comparing names ("Lock n' Load" = "lock n load").
fn norm(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    let title = t[n..].trim().trim_end_matches('#').trim();
    (n > 0 && t[n..].starts_with(' ') && !title.is_empty()).then_some((n, title))
}

fn cells(line: &str) -> Vec<&str> {
    line.split(" | ").map(str::trim).collect()
}

fn is_row(line: &str) -> bool {
    cells(line).len() >= 2
}

/// 0-based first line and end (exclusive) of a planned element's located passage.
fn span(p: &PlannedElement) -> Option<(usize, usize)> {
    let x = p.excerpt.as_ref()?;
    let s = x.start_line.checked_sub(1)?;
    Some((s, x.end_line.max(s + 1)))
}

fn failed(code: &str, message: String) -> ElementCheck {
    ElementCheck {
        code: code.into(),
        message,
        failed: true,
    }
}

fn passed(code: &str, message: &str) -> ElementCheck {
    ElementCheck {
        code: code.into(),
        message: message.into(),
        failed: false,
    }
}

fn quoted_list(names: &[String]) -> String {
    let shown: Vec<String> = names
        .iter()
        .take(MAX_NAMED)
        .map(|n| format!("\"{n}\""))
        .collect();
    match names.len().saturating_sub(MAX_NAMED) {
        0 => shown.join(", "),
        more => format!("{} and {more} more", shown.join(", ")),
    }
}

/// What the per-element checks share: the source lines, which element each table row belongs
/// to, and the rules library (numbers a source leaves to the rules).
struct Checker {
    lines: Vec<String>,
    rows: HashMap<String, Vec<usize>>,
    library: ContentStore,
    spells: Vec<(String, String)>,
}

impl Checker {
    fn new(base: &ContentStoreBuilder, draft: &ImportDraft) -> Self {
        let lines: Vec<String> = draft.source_text.lines().map(str::to_string).collect();
        let library = base.clone().build();
        Self {
            rows: rows::row_owners(draft, &lines),
            spells: numbers::spell_names(&library),
            lines,
            library,
        }
    }

    /// The numbers and table row checks of `json` as element `e` (the per-element ones).
    fn element(&self, draft: &ImportDraft, e: &DraftElement, json: &Value) -> Vec<ElementCheck> {
        let Some(p) = draft.plan.elements.iter().find(|p| p.id == e.id) else {
            return Vec::new();
        };
        let mut out = numbers::number_checks(self, draft, p, json);
        out.extend(rows::row_checks(self, draft, e, json));
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Headings

/// Most of the lines are prose (a table, or its column dumped line by line, is not).
fn prose(body: &[&str]) -> bool {
    let long = body
        .iter()
        .filter(|l| !is_row(l) && l.split_whitespace().count() >= MIN_PROSE_LINE_WORDS)
        .count();
    long * 2 >= body.len()
}

/// A heading that is not content of its own: a table fragment, the publication's legal or
/// credits pages, or a short lead-in followed by deeper sub-headings or by an element's heading
/// (title pages and forewords before the first located element are skipped by the caller).
fn structural(
    lines: &[&str],
    starts: &[usize],
    i: usize,
    (title, body_lines): (&str, &[&str]),
    words: usize,
) -> bool {
    let body = body_lines.join(" ");
    let low = format!("{} {}", title.to_lowercase(), body.to_lowercase());
    if LEGAL_WORDS.iter().any(|w| low.contains(w)) {
        return true;
    }
    if !prose(body_lines) {
        return true;
    }
    let level = heading(lines[i]).map_or(0, |(n, _)| n);
    let next = (i + 1..lines.len()).find_map(|j| heading(lines[j]).map(|(n, _)| (j, n)));
    words < MAX_INTRO_WORDS && next.is_some_and(|(j, n)| n > level || starts.contains(&j))
}

/// Every string an element carries, as plain text (a section may be carried by a field other
/// than the description: a prerequisite, a table column).
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) if !s.starts_with("ID_") => out.push(plain(s)),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(m) => m.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// Every content heading (one with a body of its own) is in an element's passage, names an
/// element, or has its body in the elements' text; the others are reported.
fn heading_checks(draft: &ImportDraft) -> Vec<ElementCheck> {
    let lines: Vec<&str> = draft.source_text.lines().collect();
    let furniture = super::furniture::furniture(&draft.source_text);
    let spans: Vec<(usize, usize)> = draft.plan.elements.iter().filter_map(span).collect();
    let starts: Vec<usize> = spans.iter().map(|(s, _)| *s).collect();
    let first = starts.iter().copied().min().unwrap_or(0);
    let names: Vec<String> = draft
        .plan
        .elements
        .iter()
        .map(|p| norm(&p.name))
        .chain(draft.plan.groups.iter().map(|g| norm(&g.name)))
        .chain(draft.detection.items.iter().map(|i| norm(&i.name)))
        .filter(|n| !n.is_empty())
        .collect();
    let mut all = Vec::new();
    for e in &draft.elements {
        strings(&e.json, &mut all);
    }
    let index = Index::new(&all.join("\n"));
    let none = HashSet::new();
    let inside = |i: usize| spans.iter().any(|(s, e)| i >= *s && i < *e);
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate().skip(first) {
        let Some((_, title)) = heading(line) else {
            continue;
        };
        let body: Vec<usize> = (i + 1..lines.len())
            .take_while(|j| heading(lines[*j]).is_none())
            .filter(|j| !lines[*j].trim().is_empty() && !furniture.has(*j))
            .collect();
        let body_lines: Vec<&str> = body.iter().map(|j| lines[*j]).collect();
        let text = body_lines.join(" ");
        let w = words(&text);
        if furniture.has(i)
            || w.len() < MIN_BODY_WORDS
            || inside(i)
            || body.iter().any(|j| inside(*j))
            || structural(&lines, &starts, i, (title, &body_lines), w.len())
        {
            continue;
        }
        let t = norm(title);
        let named = names.iter().any(|n| {
            *n == t || (n.contains(' ') && (t.contains(n.as_str()) || n.contains(t.as_str())))
        });
        if named || coverage(&w, &index, &none).is_some_and(|c| c >= BODY_COVERED) {
            continue;
        }
        out.push(failed(
            HEADING_UNCOVERED,
            format!("The section \"{title}\" was not imported"),
        ));
    }
    if out.is_empty() {
        out.push(passed(
            HEADING_UNCOVERED,
            "Every section of the source was imported",
        ));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// References

/// A missing element id as words ("ID_X_FEAT_CINDER_STEP" = "Feat Cinder Step" for prefix X).
fn readable_id(id: &str, prefix: &str) -> String {
    let rest = id.strip_prefix(prefix).unwrap_or(id);
    let rest = rest.strip_prefix("ID_").unwrap_or(rest);
    rest.trim_start_matches('_')
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let low = w.to_lowercase();
            let mut c = low.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Unresolved references per element id: the draft (with its derived fixes, as finalized)
/// loaded over the library and validated by the content store.
fn reference_failures(
    base: &ContentStoreBuilder,
    library: &ContentStore,
    draft: &ImportDraft,
) -> HashMap<String, Vec<String>> {
    let live: Vec<&DraftElement> = draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null() && e.status != ElementStatus::Failed)
        .collect();
    let docs: Vec<Value> = live.iter().map(|e| e.json.clone()).collect();
    let derived =
        super::derived::derive_with_source(&draft.plan, &docs, library, &draft.source_text);
    let doc = super::derived::content_doc(&derived);
    let Ok(file) = ContentFile::from_json_str(&doc.to_string()) else {
        return HashMap::new();
    };
    let store = base
        .clone()
        .add_file("import-selfcheck", PackOrigin::Custom, file)
        .build();
    let ids: HashSet<&str> = live.iter().map(|e| e.id.as_str()).collect();
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for issue in store.validate() {
        if issue.code != IssueCode::DanglingReference {
            continue;
        }
        let Some(id) = issue.element.as_deref().filter(|id| ids.contains(id)) else {
            continue;
        };
        let target = issue
            .message
            .rsplit_once("unknown element ")
            .map_or(issue.message.as_str(), |(_, t)| t.trim());
        let what = readable_id(target, &draft.id_prefix);
        let list = out.entry(id.to_string()).or_default();
        if !list.contains(&what) {
            list.push(what);
        }
    }
    out
}

fn reference_checks(missing: Option<&Vec<String>>) -> Vec<ElementCheck> {
    match missing {
        Some(m) if !m.is_empty() => vec![failed(
            REFERENCE_UNRESOLVED,
            format!(
                "Points at something that does not exist: {}",
                quoted_list(m)
            ),
        )],
        _ => vec![passed(REFERENCE_UNRESOLVED, "Every reference resolves")],
    }
}

// ---------------------------------------------------------------------------------------------
// Description fidelity (read-only use of the fidelity check)

/// Elements the fidelity check reads: generated ones the user has not revised (a subclass gate
/// and a located text-only element are the pipeline's own text).
fn fidelity_checked(draft: &ImportDraft, e: &DraftElement) -> bool {
    let located = draft
        .plan
        .elements
        .iter()
        .any(|p| p.id == e.id && p.excerpt.is_some());
    e.revisions == 0 && e.role != Role::SubclassGate && !(e.role == Role::Text && located)
}

fn description_checks(
    e: &DraftElement,
    index: &Index,
    mask: &HashSet<String>,
) -> Vec<ElementCheck> {
    match untraced(&e.json, index, mask).len() {
        0 => vec![passed(
            DESCRIPTION_UNTRACED,
            "The description is the source's text",
        )],
        1 => vec![failed(
            DESCRIPTION_UNTRACED,
            "One sentence of the description is not in the source".into(),
        )],
        n => vec![failed(
            DESCRIPTION_UNTRACED,
            format!("{n} sentences of the description are not in the source"),
        )],
    }
}

// ---------------------------------------------------------------------------------------------
// Running the checks

/// The source-derived checks of every generated element, and the import's heading checks.
pub(crate) fn apply(base: &ContentStoreBuilder, draft: &mut ImportDraft) {
    let checker = Checker::new(base, draft);
    apply_with(&checker, base, draft);
}

fn apply_with(checker: &Checker, base: &ContentStoreBuilder, draft: &mut ImportDraft) {
    let refs = reference_failures(base, &checker.library, draft);
    let index = Index::new(&draft.source_text);
    let mask = name_mask(draft);
    let checks: Vec<Vec<ElementCheck>> = draft
        .elements
        .iter()
        .map(|e| {
            if e.json.is_null() || e.status == ElementStatus::Failed {
                return Vec::new();
            }
            let mut out = checker.element(draft, e, &e.json);
            out.extend(reference_checks(refs.get(&e.id)));
            if fidelity_checked(draft, e) {
                out.extend(description_checks(e, &index, &mask));
            }
            out
        })
        .collect();
    for (e, c) in draft.elements.iter_mut().zip(checks) {
        e.checks = c;
    }
    draft.source_checks = heading_checks(draft);
}

// ---------------------------------------------------------------------------------------------
// Repair

/// What the model is told about a failed check.
fn feedback(c: &ElementCheck) -> String {
    let how = match c.code.as_str() {
        NUMBER_NOT_IN_SOURCE => {
            "use only the numbers the element's source text prints (dice, DCs, ranges, durations, uses, bonuses)"
        }
        TABLE_ROW_MISSING => "carry every row of the source's table, cell by cell",
        _ => "point only at ids of this import or of the rules, or leave the reference out",
    };
    format!("source check failed: {} ({how})", c.message)
}

/// Failed checks a regeneration of the element can fix (not a class or subclass root, whose
/// missing table features are missing elements, nor the pipeline's own text).
fn repairable(e: &DraftElement) -> Vec<&ElementCheck> {
    if e.json.is_null()
        || e.status == ElementStatus::Failed
        || matches!(
            e.role,
            Role::Class | Role::Subclass | Role::SubclassGate | Role::Text
        )
    {
        return Vec::new();
    }
    e.checks
        .iter()
        .filter(|c| {
            c.failed
                && matches!(
                    c.code.as_str(),
                    NUMBER_NOT_IN_SOURCE | TABLE_ROW_MISSING | REFERENCE_UNRESOLVED
                )
        })
        .collect()
}

/// Run the checks, give each element that fails a repairable one (at most
/// [`MAX_HEAL_REPAIRS`], the worst first) one repair call with the failed checks as feedback,
/// keep the answer only when it loads and fails fewer checks, re-validate without further calls
/// (an answer the sweep rejects is put back), and run the checks again. A refused call (the
/// import's call budget, a cancel) ends the repairs. Passing elements are never regenerated.
pub(crate) async fn heal(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    draft: &mut ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) {
    heal_filtered(runner, ctx, draft, None, opts, progress).await;
}

/// [`heal`] for the one element `id` (a rebuilt element): the others are only re-checked.
pub(crate) async fn heal_one(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    draft: &mut ImportDraft,
    id: &str,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) {
    heal_filtered(runner, ctx, draft, Some(id), opts, progress).await;
}

async fn heal_filtered(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    draft: &mut ImportDraft,
    only: Option<&str>,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) {
    let checker = Checker::new(&ctx.base, draft);
    apply_with(&checker, &ctx.base, draft);
    let mut todo: Vec<(String, Vec<String>, usize)> = draft
        .elements
        .iter()
        .map(|e| {
            let r = repairable(e);
            (
                e.id.clone(),
                r.iter().map(|c| feedback(c)).collect(),
                r.len(),
            )
        })
        .filter(|(id, _, n)| *n > 0 && only.map_or(true, |o| o == id))
        .collect();
    todo.sort_by_key(|t| std::cmp::Reverse(t.2));
    todo.truncate(MAX_HEAL_REPAIRS);
    let mut kept: Vec<(usize, Value, ElementStatus)> = Vec::new();
    for (k, (id, msgs, before)) in todo.iter().enumerate() {
        emit(
            progress,
            Stage::Validate,
            k,
            todo.len(),
            "source checks: repairing",
        );
        let Some(idx) = draft.elements.iter().position(|e| &e.id == id) else {
            continue;
        };
        let Some(planned) = draft.plan.elements.iter().find(|p| &p.id == id).cloned() else {
            continue;
        };
        let prev = draft.elements[idx].json.clone();
        let result = {
            let g = GenCtx {
                ctx,
                store: &checker.library,
                draft,
                parent: parent_of(draft, &planned),
            };
            let previous = Some((&prev, msgs.as_slice()));
            run_element_call(
                runner,
                &g,
                &planned,
                CHECK_REPAIR_STEP,
                previous,
                None,
                opts,
            )
            .await
        };
        let (el, notes) = match result {
            Ok((el, notes, _)) => (el, notes),
            Err(err) if refused(&err) => break,
            Err(_) => continue,
        };
        draft.elements[idx].repair_rounds += 1;
        let siblings = siblings_of(draft, id);
        let loads = element_issues(ctx, &siblings, &el, &planned, &draft.plan).is_empty();
        let after = checker
            .element(draft, &draft.elements[idx], &el)
            .iter()
            .filter(|c| c.failed)
            .count();
        if !loads || after >= *before {
            continue;
        }
        let e = &mut draft.elements[idx];
        kept.push((idx, prev, e.status));
        e.json = el;
        e.status = ElementStatus::Repaired;
        for n in notes {
            if !e.issues.contains(&n) {
                e.issues.push(n);
            }
        }
    }
    if kept.is_empty() {
        return;
    }
    validate_rounds(runner, ctx, draft, opts, progress, 0).await;
    let rejected: Vec<&(usize, Value, ElementStatus)> = kept
        .iter()
        .filter(|(idx, _, _)| draft.elements[*idx].status == ElementStatus::Failed)
        .collect();
    if !rejected.is_empty() {
        for (idx, prev, status) in rejected {
            let e = &mut draft.elements[*idx];
            e.json = prev.clone();
            e.status = *status;
        }
        validate_rounds(runner, ctx, draft, opts, progress, 0).await;
    }
    apply_with(&checker, &ctx.base, draft);
}

#[cfg(test)]
#[path = "selfcheck_tests.rs"]
mod tests;
