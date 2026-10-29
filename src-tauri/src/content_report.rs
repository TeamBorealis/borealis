//! The conversion report of an Aurora import (file, folder, .zip, or an index update): every
//! finding carries its source file, element id and type, line and a closed code, so nothing a
//! source held is lost without a precise trace.

use crate::content_dto::{MigrationIssue, MigrationReport};
use crate::content_overrides::bundled_override_issues;
use crate::content_preview::{Converted, SeverityCounts, UnresolvedRef};
use crate::error::ApiResult;
use crate::state::AppState;
use borealis_content::{
    ContentFile, ContentStore, Element, Issue, IssueCode, PackFileKind, PackManifest, Severity,
};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Most diagnostics a preview shows.
pub(crate) const MAX_DIAGNOSTICS: usize = 2_000;
/// Most issues an install report carries (a whole repository stays far below this).
pub(crate) const MAX_REPORT_ISSUES: usize = 50_000;
/// Most distinct unresolved targets sent to the UI.
const MAX_UNRESOLVED: usize = 200;
/// Referencing elements named per unresolved target.
const REFERENCED_BY_SAMPLE: usize = 3;
/// Referencing files named per unresolved target.
const FILES_SAMPLE: usize = 3;

pub(crate) const fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

/// The DTO of one migrator issue; `types` gives the element's source type.
pub(crate) fn issue_dto(issue: &Issue, types: &BTreeMap<String, String>) -> MigrationIssue {
    MigrationIssue {
        severity: severity(issue.severity),
        code: issue.code,
        message: issue.message.clone(),
        element: issue.element.clone(),
        element_type: issue.element.as_ref().and_then(|id| types.get(id).cloned()),
        file: issue.file.clone(),
        line: issue.line,
    }
}

/// Findings of one converted set, split for the preview and the install report.
pub(crate) struct Findings {
    /// Every finding except references the installed content resolves, in source order.
    pub(crate) diagnostics: Vec<MigrationIssue>,
    pub(crate) unresolved: Vec<UnresolvedRef>,
    pub(crate) unresolved_total: usize,
    /// Unknown ids only requirements name and do without (Info-level dangling references).
    pub(crate) conditions: Vec<UnresolvedRef>,
    pub(crate) conditions_total: usize,
    pub(crate) summary: SeverityCounts,
}

impl Findings {
    /// The install report (same issues as the preview, a larger cap).
    pub(crate) fn report(&self, file_name: &str, conv: &Converted) -> MigrationReport {
        MigrationReport {
            file_name: file_name.to_string(),
            format: conv.format.clone(),
            converted: conv.converted,
            skipped: conv.skipped,
            issues: self
                .diagnostics
                .iter()
                .take(MAX_REPORT_ISSUES)
                .cloned()
                .collect(),
        }
    }
}

impl AppState {
    /// Classify the issues of `conv` against the installed content and add the warnings about
    /// replaced bundled elements (one per source file and bundled pack).
    ///
    /// # Errors
    /// `internal` if the content lock is poisoned.
    pub(crate) fn findings(&self, conv: &Converted, out_dir: &Path) -> ApiResult<Findings> {
        let hub = self.hub()?;
        let mut found = classify(&conv.raw, &conv.types, |t| hub.store.contains_id(t));
        let books = book_abbreviations(&hub.store, out_dir);
        for r in found
            .unresolved
            .iter_mut()
            .chain(found.conditions.iter_mut())
        {
            r.known_source = known_book(&books, &r.target);
        }
        // review M2: an absence test of an id naming a loaded book (installed beside the set,
        // so the set's own validation could not tell) is a misspelling: unresolved
        let (misspelled, harmless): (Vec<_>, Vec<_>) = std::mem::take(&mut found.conditions)
            .into_iter()
            .partition(|r| r.known_source.is_some());
        found.conditions_total -= misspelled.len();
        found.unresolved_total += misspelled.len();
        found.conditions = harmless;
        found.unresolved.extend(misspelled);
        found
            .unresolved
            .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.target.cmp(&b.target)));
        found.unresolved.truncate(MAX_UNRESOLVED);
        if conv.converted > 0 {
            let replaced = bundled_override_issues(&hub.store, out_dir, &conv.sources);
            found.summary.warnings += replaced.len();
            found.diagnostics.extend(replaced);
        }
        Ok(found)
    }
}

/// Split issues into diagnostics and unresolved references. A reference the installed content
/// already defines (`is_installed`, the bundled packs included) is not a problem and is dropped,
/// and so is an append whose target it defines: the converter validates the new pack alone, but
/// at runtime one builder loads the bundled and installed packs first, so the append applies
/// (Aurora's `internal.xml` appends to `ID_INTERNAL_GRANTS_CHARACTER_BASE` of the core pack).
pub(crate) fn classify(
    raw: &[Issue],
    types: &BTreeMap<String, String>,
    is_installed: impl Fn(&str) -> bool,
) -> Findings {
    let mut diagnostics = Vec::new();
    let mut summary = SeverityCounts::default();
    // (target -> sample) for references something needs, and for those only conditions name
    let mut refs = RefSamples::default();
    let mut conditions = RefSamples::default();
    for issue in raw {
        if issue.code == IssueCode::DanglingReference {
            let target = issue.message.rsplit(' ').next().unwrap_or_default();
            if target.is_empty() || is_installed(target) {
                continue;
            }
            // the validator marks the references a requirement does without as Info
            let into = if issue.severity == Severity::Info {
                &mut conditions
            } else {
                &mut refs
            };
            into.add(target, issue);
            continue;
        }
        if issue.code == IssueCode::AppendTargetMissing
            && issue.element.as_deref().is_some_and(&is_installed)
        {
            continue;
        }
        match issue.severity {
            Severity::Error => summary.errors += 1,
            Severity::Warning => summary.warnings += 1,
            Severity::Info => summary.infos += 1,
        }
        diagnostics.push(issue_dto(issue, types));
    }
    // a target something needs is listed there only, even where a condition also names it
    conditions.0.retain(|t, _| !refs.0.contains_key(t));
    let (unresolved, unresolved_total) = refs.finish();
    let (conditions, conditions_total) = conditions.finish();
    Findings {
        diagnostics,
        unresolved,
        unresolved_total,
        conditions,
        conditions_total,
        summary,
    }
}

/// Unresolved targets with their use count, referencing elements and files.
#[derive(Default)]
struct RefSamples(BTreeMap<String, (usize, Vec<String>, Vec<String>)>);

impl RefSamples {
    fn add(&mut self, target: &str, issue: &Issue) {
        let slot = self.0.entry(target.to_string()).or_default();
        slot.0 += 1;
        if let Some(e) = &issue.element {
            if slot.1.len() < REFERENCED_BY_SAMPLE && !slot.1.contains(e) {
                slot.1.push(e.clone());
            }
        }
        if let Some(f) = &issue.file {
            if slot.2.len() < FILES_SAMPLE && !slot.2.contains(f) {
                slot.2.push(f.clone());
            }
        }
    }

    /// The most used first, capped at [`MAX_UNRESOLVED`], and the number of distinct targets.
    fn finish(self) -> (Vec<UnresolvedRef>, usize) {
        let total = self.0.len();
        let mut out: Vec<UnresolvedRef> = self
            .0
            .into_iter()
            .map(|(target, (count, referenced_by, files))| UnresolvedRef {
                target,
                count,
                referenced_by,
                files,
                known_source: None,
            })
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.target.cmp(&b.target)));
        out.truncate(MAX_UNRESOLVED);
        (out, total)
    }
}

/// Upper-cased `abbreviation` setter -> name of every `Source` element installed or in the set
/// written to `out_dir` (the first source of an abbreviation wins).
fn book_abbreviations(store: &ContentStore, out_dir: &Path) -> HashMap<String, String> {
    let mut books = HashMap::new();
    let mut add = |e: &Element| {
        if e.ty != "Source" {
            return;
        }
        if let Some(abbr) = e.setter_value("abbreviation").map(str::trim) {
            if !abbr.is_empty() {
                books
                    .entry(abbr.to_ascii_uppercase())
                    .or_insert_with(|| e.name.clone());
            }
        }
    };
    store.by_type("Source").for_each(|s| add(&s.element));
    for file in pack_content_files(out_dir) {
        file.elements.iter().for_each(&mut add);
    }
    books
}

/// The content files of the pack written to `out_dir` (none if it cannot be read).
fn pack_content_files(out_dir: &Path) -> Vec<ContentFile> {
    let Some(manifest) = std::fs::read_to_string(out_dir.join("pack.json"))
        .ok()
        .and_then(|t| PackManifest::from_json_str(&t).ok())
    else {
        return Vec::new();
    };
    manifest
        .files
        .iter()
        .filter(|f| f.kind == PackFileKind::Content)
        .filter_map(|f| std::fs::read_to_string(out_dir.join(&f.path)).ok())
        .filter_map(|t| ContentFile::from_json_str(&t).ok())
        .collect()
}

/// The loaded book whose abbreviation is one of the first id segments of `target`.
fn known_book(books: &HashMap<String, String>, target: &str) -> Option<String> {
    borealis_content::validate::abbreviation_segments(target)
        .find_map(|seg| books.get(&seg.to_ascii_uppercase()).cloned())
}

/// Issues of a single converted file name the file the user chose, not the scratch copy.
pub(crate) fn name_scratch_file(issues: &mut [Issue], scratch: &str, real: &str) {
    for issue in issues {
        if issue.file.as_deref() == Some(scratch) {
            issue.file = Some(real.to_string());
        }
    }
}

#[cfg(test)]
#[path = "content_report_tests.rs"]
mod tests;
