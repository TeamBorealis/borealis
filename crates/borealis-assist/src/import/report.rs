//! `report.json` (`borealis-import-report/1`): what happened to source objects that did not
//! simply become an element of the same name, plus the rule errors and normalisations of the
//! import, so that nothing is dropped or changed silently.

use serde::{Deserialize, Serialize};

/// Format tag of [`ImportReport`].
pub const REPORT_FORMAT: &str = "borealis-import-report/1";
/// File name of the report inside the pack directory.
pub const REPORT_FILE: &str = "report.json";

/// The import report written next to the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    /// Always [`REPORT_FORMAT`].
    pub format: String,
    /// One entry per source object that is not an element of the same name.
    #[serde(default)]
    pub entries: Vec<ReportEntry>,
    /// Content the engine would have ignored that survived every repair and was removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ReportIssue>,
    /// Deterministic fixes applied to generated rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub normalised: Vec<ReportIssue>,
    /// Things to check by hand (an element whose source passage was not found).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ReportIssue>,
    /// Things to check about the source as a whole, not one element (pages read visually, a
    /// page too large to read). [`ReportIssue::element`] is always an element id of the pack.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_warnings: Vec<String>,
    /// The per-element review states when the pack was made, and the user's overrides. Codes
    /// only, never source text (it goes into bug reports as it is).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewReport>,
}

/// Review summary of an import (see `anything::review`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewReport {
    /// Elements per state.
    pub ok: usize,
    /// Elements to check.
    pub check: usize,
    /// Elements not imported.
    pub not_imported: usize,
    /// Elements whose mechanics the sheet does not (fully) compute.
    pub text_only: usize,
    /// Elements the user marked as fine.
    pub accepted: usize,
    /// One entry per element that is not plainly `ok` (check, not imported, text only, accepted
    /// or with a text-only override).
    #[serde(default)]
    pub elements: Vec<ReviewReportEntry>,
    /// Failed import-level source checks (codes and counts only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub import_checks: Vec<ImportCheckReport>,
}

/// One failed import-level source check, as a code and a count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportCheckReport {
    /// The check's code.
    pub code: String,
    /// How many things failed it.
    pub count: usize,
}

/// One element's review state, as codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewReportEntry {
    /// Element id in the pack.
    pub element: String,
    /// `ok`, `check` or `not_imported`.
    pub state: String,
    /// Code of the leading reason.
    pub code: String,
    /// The sheet does not compute some of its mechanics.
    #[serde(default)]
    pub text_only: bool,
    /// The user's text-only override (stored in the pack; the sheet can show it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_only_override: Option<bool>,
    /// The user marked it as fine.
    #[serde(default)]
    pub accepted: bool,
}

impl Default for ImportReport {
    fn default() -> Self {
        Self {
            format: REPORT_FORMAT.to_string(),
            entries: Vec::new(),
            errors: Vec::new(),
            normalised: Vec::new(),
            warnings: Vec::new(),
            source_warnings: Vec::new(),
            review: None,
        }
    }
}

/// One source object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportEntry {
    /// Name as printed in the source.
    pub source: String,
    /// `element` (renamed element), `merged`, `text_only`, `unsupported`, `skipped`, `dropped`
    /// (an element that failed and was left out) or `unchecked` (a failed element kept for review).
    pub outcome: String,
    /// Element ids of the pack that carry it.
    #[serde(default)]
    pub elements: Vec<String>,
    /// Why.
    #[serde(default)]
    pub note: String,
}

/// One message about one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportIssue {
    /// Element id in the pack.
    pub element: String,
    /// What happened.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_warnings_are_kept_apart_from_element_warnings() {
        let empty = serde_json::to_value(ImportReport::default()).unwrap();
        assert!(empty.get("source_warnings").is_none(), "{empty}");
        let mut r = ImportReport::default();
        r.source_warnings
            .push("2 page(s) were read visually".into());
        let back: ImportReport = serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(back.warnings.is_empty());
        // Reports written before the field existed still load.
        let old: ImportReport =
            serde_json::from_str(r#"{"format":"borealis-import-report/1"}"#).unwrap();
        assert!(old.source_warnings.is_empty());
    }
}
