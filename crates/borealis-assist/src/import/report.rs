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
        }
    }
}

/// One source object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportEntry {
    /// Name as printed in the source.
    pub source: String,
    /// `element` (renamed element), `merged`, `text_only`, `unsupported` or `skipped`.
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
