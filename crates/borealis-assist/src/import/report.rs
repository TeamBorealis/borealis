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
}

impl Default for ImportReport {
    fn default() -> Self {
        Self {
            format: REPORT_FORMAT.to_string(),
            entries: Vec::new(),
            errors: Vec::new(),
            normalised: Vec::new(),
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
