//! Stage 7: the draft becomes a `user-import` pack with deterministic ids and no collisions.

use std::collections::HashSet;

use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::{ContentStoreBuilder, Severity};
use serde_json::Value;

use super::derived::{content_doc, derive_with_source};
use super::types::{DraftElement, ElementStatus, ImportDraft};
use crate::error::AssistError;
use crate::import::ids::make_id;
use crate::import::pack::{assemble, ImportedPack};
use crate::import::report::{ImportReport, ReportEntry, ReportIssue};

/// A finished pack plus the ids that had to change.
#[derive(Debug, Clone, PartialEq)]
pub struct Finalized {
    /// The pack (`origin: "user-import"`, `redistributable: false`).
    pub pack: ImportedPack,
    /// `(old, new)` for ids that collided with the library.
    pub renamed: Vec<(String, String)>,
}

/// Longest failure reason copied into the report.
const REASON_MAX_CHARS: usize = 300;

/// Build the pack from a reviewed draft.
///
/// One bad element never costs the import: an element with no usable output (generation or
/// repair failed) is left out, every grant, select or list pointing at it is removed so nothing
/// dangles, and the report gets one `dropped` entry for it. A `Failed` element that still has
/// output (a sweep check it never passed) is kept for review, unless the pack does not load
/// with it; then it is left out the same way.
///
/// # Errors
/// [`AssistError::Invalid`] while elements are pending (the import has not finished), or when
/// the pack does not load cleanly on top of `base` for a reason no left-out element explains.
pub fn finalize_draft(
    base: &ContentStoreBuilder,
    draft: &ImportDraft,
) -> Result<Finalized, AssistError> {
    let pending: Vec<String> = draft
        .elements
        .iter()
        .filter(|e| e.status == ElementStatus::Pending)
        .map(|e| e.name.clone())
        .collect();
    if !pending.is_empty() {
        return Err(AssistError::Invalid(format!(
            "cannot commit: {} element(s) are not generated yet: {}",
            pending.len(),
            pending.join(", ")
        )));
    }
    let mut left_out: HashSet<String> = draft
        .elements
        .iter()
        .filter(|e| e.json.is_null())
        .map(|e| e.id.clone())
        .collect();
    loop {
        let errors = match build_pack(base, draft, &left_out)? {
            Ok(done) => return Ok(done),
            Err(errors) => errors,
        };
        // A kept failed element that breaks the pack is left out too; anything else is a bug.
        let culprits: Vec<String> = errors
            .iter()
            .filter_map(|(id, _)| draft.element(id))
            .filter(|e| e.status == ElementStatus::Failed && !left_out.contains(&e.id))
            .map(|e| e.id.clone())
            .collect();
        if culprits.is_empty() {
            let messages: Vec<&str> = errors.iter().map(|(_, m)| m.as_str()).collect();
            return Err(AssistError::Invalid(format!(
                "the pack has errors: {}",
                messages.join("; ")
            )));
        }
        left_out.extend(culprits);
    }
}

/// `(draft element id, message)` of the pack's load errors.
type PackErrors = Vec<(String, String)>;

/// The pack without the `left_out` elements, or its load errors.
fn build_pack(
    base: &ContentStoreBuilder,
    draft: &ImportDraft,
    left_out: &HashSet<String>,
) -> Result<Result<Finalized, PackErrors>, AssistError> {
    let library_store = base.clone().build();
    let library: HashSet<String> = library_store.iter().map(|s| s.element.id.clone()).collect();
    let mut used: HashSet<String> = library.clone();
    used.extend(draft.elements.iter().map(|e| e.id.clone()));
    let kept: Vec<&DraftElement> = draft
        .elements
        .iter()
        .filter(|e| !left_out.contains(&e.id))
        .collect();
    let mut raw: Vec<Value> = kept.iter().map(|e| e.json.clone()).collect();
    let mut local = tiered_spell_choices(&mut raw, draft);
    local.extend(printed_fixes(&mut raw, draft));
    let mut derived = derive_with_source(&draft.plan, &raw, &library_store, &draft.source_text);
    let (markers, notes) =
        super::fallback::skill_fallbacks(&mut derived.docs, &draft.id_prefix, &mut used);
    derived.extra.extend(markers);
    local.extend(notes);
    derived.notes.extend(local);
    let (mut doc, removed) = without_references(&content_doc(&derived), left_out);
    let mut renamed = Vec::new();
    for e in &kept {
        if !library.contains(&e.id) {
            continue;
        }
        let new = make_id(&draft.id_prefix, &e.ty, &e.name, &used);
        used.insert(new.clone());
        renamed.push((e.id.clone(), new));
    }
    if !renamed.is_empty() {
        let mut text = doc.to_string();
        for (old, new) in &renamed {
            text = text.replace(&format!("\"{old}\""), &format!("\"{new}\""));
        }
        doc = serde_json::from_str(&text).map_err(|e| AssistError::Invalid(e.to_string()))?;
    }
    let file = ContentFile::from_json_str(&doc.to_string())
        .map_err(|e| AssistError::Invalid(format!("the draft does not load: {e}")))?;
    let ids: HashSet<String> = file.elements.iter().map(|e| e.id.clone()).collect();
    let store = base
        .clone()
        .add_file("import-final", PackOrigin::Custom, file.clone())
        .build();
    let original = |id: &str| {
        renamed
            .iter()
            .find(|(_, new)| new == id)
            .map_or_else(|| id.to_string(), |(old, _)| old.clone())
    };
    let errors: PackErrors = store
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .filter_map(|i| {
            let id = i.element.filter(|id| ids.contains(id))?;
            Some((original(&id), i.message))
        })
        .collect();
    if !errors.is_empty() {
        return Ok(Err(errors));
    }
    let mut pack = assemble(
        &draft.pack_id,
        &draft.pack_name,
        draft.edition.ruleset(),
        &draft.source_kind,
        file,
    );
    let mut report = build_report(draft, &renamed, left_out);
    let final_id = |id: &str| {
        renamed
            .iter()
            .find(|(old, _)| old == id)
            .map_or_else(|| id.to_string(), |(_, new)| new.clone())
    };
    report
        .entries
        .extend(left_out_entries(draft, left_out, &removed, &final_id));
    report
        .normalised
        .extend(derived.notes.iter().map(|(id, m)| ReportIssue {
            element: final_id(id),
            message: m.clone(),
        }));
    pack.report = Some(report);
    Ok(Ok(Finalized { pack, renamed }))
}

/// Report entries for the failed elements: `dropped` for each left-out one (with how many
/// references to it were removed) and `unchecked` for each one kept with output it never got
/// past the engine checks. The bug report counts both as failed self-checks.
fn left_out_entries(
    draft: &ImportDraft,
    left_out: &HashSet<String>,
    removed: &[(String, usize)],
    final_id: &dyn Fn(&str) -> String,
) -> Vec<ReportEntry> {
    let reason = |e: &DraftElement| -> String {
        e.issues
            .first()
            .map_or("no usable element was generated", String::as_str)
            .chars()
            .take(REASON_MAX_CHARS)
            .collect()
    };
    draft
        .elements
        .iter()
        .filter(|e| left_out.contains(&e.id) || e.status == ElementStatus::Failed)
        .map(|e| {
            if !left_out.contains(&e.id) {
                return ReportEntry {
                    source: e.name.clone(),
                    outcome: UNCHECKED.into(),
                    elements: vec![final_id(&e.id)],
                    note: format!(
                        "{} kept although it never passed the engine checks ({}); check it by hand.",
                        e.ty,
                        reason(e)
                    ),
                };
            }
            let refs = match removed.iter().find(|(id, _)| *id == e.id) {
                Some((_, n)) => format!(" {n} reference(s) to it were removed."),
                None => String::new(),
            };
            ReportEntry {
                source: e.name.clone(),
                outcome: DROPPED.into(),
                elements: Vec::new(),
                note: format!(
                    "{} left out of the pack: it failed generation or every repair ({}).{refs} \
Revise the draft to bring it back.",
                    e.ty,
                    reason(e)
                ),
            }
        })
        .collect()
}

/// Report outcome of a failed element kept for review.
pub(crate) const UNCHECKED: &str = "unchecked";
/// Report outcome of an element left out of the pack.
pub(crate) const DROPPED: &str = "dropped";

/// `doc` with every rule that points at a left-out element removed, and every left-out id taken
/// out of id lists (`a|b` supports, arrays of ids); a rule whose list becomes empty goes too.
/// Returns the new document and `(left-out id, references removed)`.
fn without_references(doc: &Value, left_out: &HashSet<String>) -> (Value, Vec<(String, usize)>) {
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut hit = |id: &str| match counts.iter_mut().find(|(i, _)| i == id) {
        Some((_, n)) => *n += 1,
        None => counts.push((id.to_string(), 1)),
    };
    let out = scrub(doc, left_out, &mut hit);
    (out, counts)
}

fn scrub(v: &Value, left_out: &HashSet<String>, hit: &mut dyn FnMut(&str)) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, x)| {
                    let x = if k == "rules" {
                        scrub_rules(x, left_out, hit)
                    } else {
                        scrub(x, left_out, hit)
                    };
                    (k.clone(), x)
                })
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.iter().map(|x| scrub(x, left_out, hit)).collect())
        }
        other => other.clone(),
    }
}

fn scrub_rules(rules: &Value, left_out: &HashSet<String>, hit: &mut dyn FnMut(&str)) -> Value {
    let Some(items) = rules.as_array() else {
        return rules.clone();
    };
    Value::Array(
        items
            .iter()
            .filter_map(|r| scrub_rule(r, left_out, hit))
            .collect(),
    )
}

/// The rule without its references to left-out elements, or None when nothing of it is left.
fn scrub_rule(
    rule: &Value,
    left_out: &HashSet<String>,
    hit: &mut dyn FnMut(&str),
) -> Option<Value> {
    let Some(map) = rule.as_object() else {
        return Some(rule.clone());
    };
    let mut out = map.clone();
    for (k, v) in map {
        match v {
            Value::String(s) if left_out.contains(s) => {
                hit(s);
                return None;
            }
            Value::String(s) if s.contains('|') => {
                let parts: Vec<&str> = s.split('|').collect();
                let kept: Vec<&str> = parts
                    .iter()
                    .copied()
                    .filter(|p| !left_out.contains(p.trim()))
                    .collect();
                if kept.len() == parts.len() {
                    continue;
                }
                parts
                    .iter()
                    .filter(|p| left_out.contains(p.trim()))
                    .for_each(|p| hit(p.trim()));
                if kept.is_empty() {
                    return None;
                }
                out.insert(k.clone(), Value::String(kept.join("|")));
            }
            Value::Array(a)
                if a.iter()
                    .any(|x| x.as_str().is_some_and(|s| left_out.contains(s))) =>
            {
                let kept: Vec<Value> = a
                    .iter()
                    .filter(|x| {
                        let gone = x.as_str().filter(|s| left_out.contains(*s));
                        gone.iter().for_each(|s| hit(s));
                        gone.is_none()
                    })
                    .cloned()
                    .collect();
                if kept.is_empty() {
                    return None;
                }
                out.insert(k.clone(), Value::Array(kept));
            }
            _ => {}
        }
    }
    Some(Value::Object(out))
}

/// Tiered free-cast features of drafts written before the selects were added at generation
/// (see [`super::arcanum`]); a no-op on current drafts.
fn tiered_spell_choices(raw: &mut [Value], draft: &ImportDraft) -> Vec<(String, String)> {
    raw.iter_mut()
        .filter(|el| {
            matches!(
                el.get("type").and_then(Value::as_str),
                Some("Class Feature" | "Archetype Feature")
            )
        })
        .filter_map(|el| {
            let note = super::arcanum::with_selects(el, draft.edition)?;
            let id = el.get("id").and_then(Value::as_str)?.to_string();
            Some((id, note))
        })
        .collect()
}

/// Deterministic fixes applied at generation, repeated for older drafts: the attunement
/// restriction in both the setter and the text ([`super::mechanics::attunement_by`]), the
/// class spell save DC a feature's save uses ([`super::mechanics::caster_save_dc`]) and a
/// magic armor's enhancement bonus ([`super::items::armor_enhancement`]). A table-row
/// item's column lines are its printed row ([`super::items::printed_row_fixes`]).
fn printed_fixes(raw: &mut [Value], draft: &ImportDraft) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for el in raw.iter_mut() {
        let id = el
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut notes = super::mechanics::attunement_by(el);
        notes.extend(super::mechanics::caster_save_dc(el));
        let planned = draft.plan.elements.iter().find(|p| p.id == id);
        notes.extend(super::items::printed_row_fixes(
            el,
            planned,
            &draft.source_text,
        ));
        if el.get("type").and_then(Value::as_str) == Some("Magic Item") {
            notes.extend(super::items::armor_enhancement(el));
        }
        out.extend(notes.into_iter().map(|n| (id.clone(), n)));
    }
    out
}

fn same_name(a: &str, b: &str) -> bool {
    let n = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    n(a) == n(b)
}

/// The source object `name` was imported: a plan group or an element in the pack has its name
/// (a table's rows planned as options or items, whatever detect called them), an element's
/// description prints it as a heading, run-in or table row (a table or sub-heading folded into
/// a text entry), or every row of its table is an element in the pack.
fn imported_as_named(draft: &ImportDraft, left_out: &HashSet<String>, name: &str) -> bool {
    let kept = || draft.elements.iter().filter(|e| !left_out.contains(&e.id));
    draft.plan.groups.iter().any(|g| same_name(&g.name, name))
        || kept().any(|e| same_name(&e.name, name))
        || kept().any(|e| {
            e.json
                .get("description")
                .and_then(Value::as_str)
                .is_some_and(|d| printed_in(d, name))
        })
        || rows_imported(draft, left_out, name)
}

/// The text runs of `html` between its tags.
fn text_runs(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut tag = false;
    for (i, c) in html.char_indices() {
        match c {
            '<' if !tag => {
                out.push(&html[start..i]);
                tag = true;
            }
            '>' if tag => {
                tag = false;
                start = i + 1;
            }
            _ => {}
        }
    }
    if !tag {
        out.push(&html[start..]);
    }
    out
}

/// `html` prints `name` as a line of its own (a heading, a run-in label) or as a table row's
/// first cell ("Name | ...", or a `<td>`).
fn printed_in(html: &str, name: &str) -> bool {
    text_runs(html).into_iter().any(|run| {
        let line = run.trim().trim_end_matches(['.', ':']).trim_end();
        let first = run.split('|').next().unwrap_or("");
        same_name(line, name) || (run.contains('|') && same_name(first.trim(), name))
    })
}

/// Most lines below a table's caption searched for its rows.
const TABLE_SCAN_LINES: usize = 120;

/// The source prints `name` as a caption line, and every row of the table under it (to the
/// next heading; its first `|` line is the header) is an element in the pack.
fn rows_imported(draft: &ImportDraft, left_out: &HashSet<String>, name: &str) -> bool {
    let names: Vec<&str> = draft
        .elements
        .iter()
        .filter(|e| !left_out.contains(&e.id))
        .map(|e| e.name.as_str())
        .collect();
    let row_named = |cell: &str| {
        let cell = cell.trim();
        names.iter().any(|n| {
            same_name(cell, n)
                || cell
                    .to_lowercase()
                    .strip_prefix(&n.to_lowercase())
                    .is_some_and(|rest| rest.starts_with(" ("))
        })
    };
    let lines: Vec<&str> = draft.source_text.lines().collect();
    lines.iter().enumerate().any(|(k, line)| {
        if !same_name(line.trim_start_matches('#').trim(), name) {
            return false;
        }
        let rows: Vec<&str> = lines[k + 1..]
            .iter()
            .take(TABLE_SCAN_LINES)
            .take_while(|l| !l.trim_start().starts_with('#'))
            .filter(|l| l.contains('|'))
            .skip(1)
            .copied()
            .collect();
        !rows.is_empty()
            && rows
                .iter()
                .all(|r| row_named(r.split('|').next().unwrap_or("")))
    })
}

/// The import report: source objects with no element of their name, the rule errors that
/// survived every repair (removed from the pack) and the deterministic rule fixes.
pub(crate) fn build_report(
    draft: &ImportDraft,
    renamed: &[(String, String)],
    left_out: &HashSet<String>,
) -> ImportReport {
    let final_id = |id: &str| {
        renamed
            .iter()
            .find(|(old, _)| old == id)
            .map_or_else(|| id.to_string(), |(_, new)| new.clone())
    };
    let mut report = ImportReport::default();
    for g in &draft.plan.groups {
        let Some(root) = g.root_id.as_deref().and_then(|id| draft.element(id)) else {
            report.entries.push(ReportEntry {
                source: g.name.clone(),
                outcome: "skipped".into(),
                elements: Vec::new(),
                note: "no element was planned for it".into(),
            });
            continue;
        };
        if !same_name(&root.name, &g.name) {
            report.entries.push(ReportEntry {
                source: g.name.clone(),
                outcome: "element".into(),
                elements: vec![final_id(&root.id)],
                note: format!("imported as \"{}\"", root.name),
            });
        }
    }
    report.entries.extend(new_properties(draft, &final_id));
    report
        .entries
        .extend(super::parts::report_entries(draft, &final_id));
    for o in draft
        .detection
        .other
        .iter()
        .filter(|o| !imported_as_named(draft, left_out, &o.name))
    {
        let what = if o.what.is_empty() {
            "this"
        } else {
            o.what.as_str()
        };
        let note = format!("no element kind for {what}; not imported. {}", o.note);
        report.entries.push(ReportEntry {
            source: o.name.clone(),
            outcome: "unsupported".into(),
            elements: Vec::new(),
            note: note.trim().to_string(),
        });
    }
    for e in draft.elements.iter().filter(|e| !left_out.contains(&e.id)) {
        let id = final_id(&e.id);
        let issue = |m: &String| ReportIssue {
            element: id.clone(),
            message: m.clone(),
        };
        report.errors.extend(e.rule_errors.iter().map(issue));
        report.normalised.extend(e.normalised.iter().map(issue));
    }
    for p in draft
        .plan
        .elements
        .iter()
        .filter(|p| !left_out.contains(&p.id))
    {
        if let Some(message) = super::batch::unlocated_warning(&draft.source_text, p) {
            report.warnings.push(ReportIssue {
                element: final_id(&p.id),
                message,
            });
        }
    }
    // What could not be read from the attachments (pages read visually, oversized pages,
    // untranscribed headings): about the source, not one element.
    report.source_warnings.extend(
        draft
            .detection
            .notes
            .iter()
            .filter_map(|n| n.strip_prefix(super::ingest::READING_NOTE))
            .map(str::to_string),
    );
    report
}

/// Weapon properties no bundled tag covers: kept as named properties (with the source's text
/// when it defines them), never dropped, and reported because the engine applies no rules.
fn new_properties(draft: &ImportDraft, final_id: &dyn Fn(&str) -> String) -> Vec<ReportEntry> {
    let mut by_name: Vec<(String, Vec<String>)> = Vec::new();
    for e in draft.elements.iter().filter(|e| e.ty == "Weapon") {
        for p in super::items::weapon_properties(&e.json) {
            if super::items::known_property(&p).is_some() {
                continue;
            }
            let key = p.split('(').next().unwrap_or(&p).trim().to_string();
            match by_name.iter_mut().find(|(n, _)| same_name(n, &key)) {
                Some((_, users)) => users.push(e.name.clone()),
                None => by_name.push((key, vec![e.name.clone()])),
            }
        }
    }
    by_name
        .into_iter()
        .map(|(name, users)| {
            let defined = draft
                .elements
                .iter()
                .find(|e| e.ty == "Weapon Property" && same_name(&e.name, &name));
            let on = users.join(", ");
            match defined {
                Some(p) => ReportEntry {
                    source: name,
                    outcome: "text_only".into(),
                    elements: vec![final_id(&p.id)],
                    note: format!("new weapon property, imported with its description; the engine applies no rules for it (on {on})"),
                },
                None => ReportEntry {
                    source: name,
                    outcome: "text_only".into(),
                    elements: draft
                        .elements
                        .iter()
                        .filter(|e| e.ty == "Weapon" && users.contains(&e.name))
                        .map(|e| final_id(&e.id))
                        .collect(),
                    note: format!("weapon property the source does not define; kept by name on {on}, no rules"),
                },
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "finalize_tests.rs"]
mod tests;
