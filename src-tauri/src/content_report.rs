//! The conversion report of an Aurora import (file, folder, .zip, or an index update): every
//! finding carries its source file, element id and type, line and a closed code, so nothing a
//! source held is lost without a precise trace.

use crate::content_dto::{MigrationIssue, MigrationReport};
use crate::content_overrides::bundled_override_issues;
use crate::content_preview::{Converted, SeverityCounts, UnresolvedRef};
use crate::error::ApiResult;
use crate::state::AppState;
use borealis_content::{Issue, IssueCode, Severity};
use std::collections::BTreeMap;
use std::path::Path;

/// Most diagnostics a preview shows.
pub(crate) const MAX_DIAGNOSTICS: usize = 2_000;
/// Most issues an install report carries (a whole repository stays far below this).
pub(crate) const MAX_REPORT_ISSUES: usize = 50_000;
/// Most distinct unresolved targets sent to the UI.
const MAX_UNRESOLVED: usize = 200;
/// Referencing elements named per unresolved target.
const REFERENCED_BY_SAMPLE: usize = 3;

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
        if conv.converted > 0 {
            let replaced = bundled_override_issues(&hub.store, out_dir, &conv.sources);
            found.summary.warnings += replaced.len();
            found.diagnostics.extend(replaced);
        }
        Ok(found)
    }
}

/// Split issues into diagnostics and unresolved references. A reference the installed content
/// already defines (`is_installed`) is not a problem and is dropped.
pub(crate) fn classify(
    raw: &[Issue],
    types: &BTreeMap<String, String>,
    is_installed: impl Fn(&str) -> bool,
) -> Findings {
    let mut diagnostics = Vec::new();
    let mut summary = SeverityCounts::default();
    let mut refs: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for issue in raw {
        if issue.code == IssueCode::DanglingReference {
            let target = issue.message.rsplit(' ').next().unwrap_or_default();
            if target.is_empty() || is_installed(target) {
                continue;
            }
            let slot = refs.entry(target.to_string()).or_default();
            slot.0 += 1;
            if let Some(e) = &issue.element {
                if slot.1.len() < REFERENCED_BY_SAMPLE && !slot.1.contains(e) {
                    slot.1.push(e.clone());
                }
            }
            continue;
        }
        match issue.severity {
            Severity::Error => summary.errors += 1,
            Severity::Warning => summary.warnings += 1,
            Severity::Info => summary.infos += 1,
        }
        diagnostics.push(issue_dto(issue, types));
    }
    let unresolved_total = refs.len();
    let mut unresolved: Vec<UnresolvedRef> = refs
        .into_iter()
        .map(|(target, (count, referenced_by))| UnresolvedRef {
            target,
            count,
            referenced_by,
        })
        .collect();
    unresolved.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.target.cmp(&b.target)));
    unresolved.truncate(MAX_UNRESOLVED);
    Findings {
        diagnostics,
        unresolved,
        unresolved_total,
        summary,
    }
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
