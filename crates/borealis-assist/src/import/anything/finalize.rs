//! Stage 7: the draft becomes a `user-import` pack with deterministic ids and no collisions.

use std::collections::HashSet;

use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::{ContentStoreBuilder, Severity};
use serde_json::Value;

use super::derived::{content_doc, derive};
use super::types::{ElementStatus, ImportDraft};
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

/// Build the pack from a reviewed draft.
///
/// # Errors
/// [`AssistError::Invalid`] while elements are pending or failed, or when the assembled pack does
/// not load cleanly on top of `base`.
pub fn finalize_draft(
    base: &ContentStoreBuilder,
    draft: &ImportDraft,
) -> Result<Finalized, AssistError> {
    let bad: Vec<String> = draft
        .elements
        .iter()
        .filter(|e| {
            matches!(e.status, ElementStatus::Pending | ElementStatus::Failed) || e.json.is_null()
        })
        .map(|e| format!("{} ({:?})", e.name, e.status))
        .collect();
    if !bad.is_empty() {
        return Err(AssistError::Invalid(format!(
            "cannot commit: {} element(s) are not ready: {}",
            bad.len(),
            bad.join(", ")
        )));
    }
    let library_store = base.clone().build();
    let library: HashSet<String> = library_store.iter().map(|s| s.element.id.clone()).collect();
    let mut used: HashSet<String> = library.clone();
    used.extend(draft.elements.iter().map(|e| e.id.clone()));
    let raw: Vec<Value> = draft.elements.iter().map(|e| e.json.clone()).collect();
    let derived = derive(&draft.plan, &raw, &library_store);
    let mut doc = content_doc(&derived);
    let mut renamed = Vec::new();
    for e in &draft.elements {
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
    let errors: Vec<String> = store
        .validate()
        .into_iter()
        .filter(|i| {
            i.severity == Severity::Error && i.element.as_ref().is_some_and(|id| ids.contains(id))
        })
        .map(|i| i.message)
        .collect();
    if !errors.is_empty() {
        return Err(AssistError::Invalid(format!(
            "the pack has errors: {}",
            errors.join("; ")
        )));
    }
    let mut pack = assemble(
        &draft.pack_id,
        &draft.pack_name,
        draft.edition.ruleset(),
        &draft.source_kind,
        file,
    );
    let mut report = build_report(draft, &renamed);
    report
        .normalised
        .extend(derived.notes.iter().map(|(id, m)| {
            ReportIssue {
                element: renamed
                    .iter()
                    .find(|(old, _)| old == id)
                    .map_or_else(|| id.clone(), |(_, new)| new.clone()),
                message: m.clone(),
            }
        }));
    pack.report = Some(report);
    Ok(Finalized { pack, renamed })
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

/// The import report: source objects with no element of their name, the rule errors that
/// survived every repair (removed from the pack) and the deterministic rule fixes.
pub(crate) fn build_report(draft: &ImportDraft, renamed: &[(String, String)]) -> ImportReport {
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
    for o in draft.detection.other.iter().filter(|o| {
        !draft
            .plan
            .groups
            .iter()
            .any(|g| same_name(&g.name, &o.name))
    }) {
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
    for e in &draft.elements {
        let id = final_id(&e.id);
        let issue = |m: &String| ReportIssue {
            element: id.clone(),
            message: m.clone(),
        };
        report.errors.extend(e.rule_errors.iter().map(issue));
        report.normalised.extend(e.normalised.iter().map(issue));
    }
    for p in &draft.plan.elements {
        if let Some(message) = super::batch::unlocated_warning(&draft.source_text, p) {
            report.warnings.push(ReportIssue {
                element: final_id(&p.id),
                message,
            });
        }
    }
    // What could not be read from the attachments (oversized pages, untranscribed headings).
    for n in &draft.detection.notes {
        if let Some(message) = n.strip_prefix(super::ingest::READING_NOTE) {
            report.warnings.push(ReportIssue {
                element: "source".into(),
                message: message.to_string(),
            });
        }
    }
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
