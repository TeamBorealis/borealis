//! A problem of a draft as the creator shows it (spec 8.2, 8.3): a plain sentence, a severity, the form path of
//! the field it is about, the element it belongs to, and a machine-readable code.

use serde::{Deserialize, Serialize};

/// How serious a [`Problem`] is. Errors block Save (a pack must always load); warnings and info never do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProblemSeverity {
    /// The element can't be saved like this.
    Error,
    /// It saves and loads, but something is likely wrong.
    Warning,
    /// A hint (balance comparisons, an empty description).
    Info,
}

impl ProblemSeverity {
    /// From a content validator severity.
    #[must_use]
    pub fn of_issue(s: borealis_content::Severity) -> Self {
        match s {
            borealis_content::Severity::Error => Self::Error,
            borealis_content::Severity::Warning => Self::Warning,
            borealis_content::Severity::Info => Self::Info,
        }
    }
}

/// One problem (`HomebrewProblem` on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// Severity.
    pub severity: ProblemSeverity,
    /// One plain sentence: no ids, no field keys, no jargon.
    pub message: String,
    /// Form path of the field (`fields.name`, `blocks.2.fields.number`, `children.0`); `None` = the element.
    pub path: Option<String>,
    /// The element it is about (a class draft has features); `None` for a new element without an id.
    pub element_id: Option<String>,
    /// Machine-readable code: an `IssueCode` (`dangling_reference`), a rule problem (`content.unknown_rule_kind`),
    /// or the creator's own (`form.required`, `block.required`, `lint.escape`, `sweep.empty_select`,
    /// `engine.refused`, `balance.feat.asi_total`).
    pub code: String,
}

impl Problem {
    /// A problem; an empty path means the element itself.
    #[must_use]
    pub fn new(
        severity: ProblemSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
        path: impl Into<String>,
        element_id: Option<&str>,
    ) -> Self {
        let path = path.into();
        Self {
            severity,
            message: message.into(),
            path: (!path.is_empty()).then_some(path),
            element_id: element_id.map(str::to_string),
            code: code.into(),
        }
    }

    /// True for an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == ProblemSeverity::Error
    }
}

/// `problems` without repeats: one problem per (element, path, severity) and message; the errors of a field come from
/// one pipeline step only, the earliest (a required field is told by the form, not again by the content validator).
#[must_use]
pub fn dedupe(problems: Vec<Problem>) -> Vec<Problem> {
    let mut seen = std::collections::HashSet::new();
    let mut errored: std::collections::HashMap<(Option<String>, Option<String>), String> =
        std::collections::HashMap::new();
    problems
        .into_iter()
        .filter(|p| {
            let at = (p.element_id.clone(), p.path.clone());
            if p.is_error() && p.path.is_some() {
                let first = errored.entry(at.clone()).or_insert_with(|| p.code.clone());
                if *first != p.code {
                    return false;
                }
            }
            seen.insert((at, p.severity, p.message.clone()))
        })
        .collect()
}
