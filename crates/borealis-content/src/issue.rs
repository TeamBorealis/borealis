//! Diagnostics shared by the store, the validator and the migrator.

use serde::{Deserialize, Serialize};

/// How serious an [`Issue`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational (normalisations, notes).
    Info,
    /// Content works but is suspicious.
    Warning,
    /// Content is wrong; the affected item is dropped or unusable.
    Error,
}

/// Machine readable issue category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    // --- XML / conversion (borealis-migrate) ---
    /// The XML is not well formed.
    XmlParse,
    /// A required attribute is missing.
    MissingAttribute,
    /// An integer attribute could not be parsed.
    InvalidInteger,
    /// A boolean attribute could not be parsed.
    InvalidBoolean,
    /// A version string is invalid.
    InvalidVersion,
    /// A rule tag is not grant/select/stat.
    UnknownRuleTag,
    /// A construct that has no effect in the legacy format was preserved or dropped.
    UnsupportedConstruct,
    /// An element could not be converted and was dropped.
    ElementDropped,
    /// A singleton child appeared more than once; the extras were kept in `extra`.
    DuplicateChild,
    /// A legacy spelling was normalised.
    Normalized,
    /// An index entry names a file that is not present locally.
    IndexEntryMissing,
    /// A file was skipped (ignored path, unsupported extension, `ignore="true"`).
    FileSkipped,
    // --- content (borealis-content) ---
    /// Two elements share an id.
    DuplicateId,
    /// A later definition replaced an earlier one.
    ElementReplaced,
    /// An id contains characters that are operators in the expression languages.
    InvalidId,
    /// An id does not follow the `ID_UPPER_CASE` convention.
    NonStandardId,
    /// A required setter (or setter attribute) is missing.
    MissingRequiredSetter,
    /// A referenced element id does not exist in the loaded universe.
    DanglingReference,
    /// An expression could not be parsed.
    InvalidExpression,
    /// A multiclass node has the same id as its class.
    MulticlassIdSameAsClass,
    /// An `<append>` targets an unknown element.
    AppendTargetMissing,
    /// Two selects of one element share a name.
    DuplicateSelectName,
    /// An element names a source that no Source element defines.
    UndefinedSource,
    /// The JSON does not conform to the schema.
    SchemaViolation,
    /// A Monster element's `data` is unreadable or internally inconsistent.
    InvalidMonsterData,
}

/// One diagnostic with enough context to locate it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// Severity.
    pub severity: Severity,
    /// Category.
    pub code: IssueCode,
    /// Human readable message.
    pub message: String,
    /// The file the issue concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The element id the issue concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    /// A path inside the element such as `rules[3]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 1-based source line for XML issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl Issue {
    /// A new issue without location.
    #[must_use]
    pub fn new(severity: Severity, code: IssueCode, message: impl Into<String>) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            file: None,
            element: None,
            path: None,
            line: None,
        }
    }

    /// Shorthand for an error.
    #[must_use]
    pub fn error(code: IssueCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }

    /// Shorthand for a warning.
    #[must_use]
    pub fn warning(code: IssueCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }

    /// Shorthand for an informational note.
    #[must_use]
    pub fn info(code: IssueCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, code, message)
    }

    /// Attach a file name.
    #[must_use]
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Attach an element id.
    #[must_use]
    pub fn at_element(mut self, id: impl Into<String>) -> Self {
        self.element = Some(id.into());
        self
    }

    /// Attach a path inside the element.
    #[must_use]
    pub fn at_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Attach a source line.
    #[must_use]
    pub fn at_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {:?}", self.severity, self.code)?;
        if let Some(file) = &self.file {
            write!(f, " {file}")?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
        }
        if let Some(id) = &self.element {
            write!(f, " [{id}]")?;
        }
        if let Some(path) = &self.path {
            write!(f, " {path}")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Count issues of one severity.
#[must_use]
pub fn count_severity(issues: &[Issue], severity: Severity) -> usize {
    issues.iter().filter(|i| i.severity == severity).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_and_display() {
        let issue = Issue::warning(IssueCode::DanglingReference, "no such id")
            .in_file("a.json")
            .at_element("ID_X")
            .at_path("rules[2]")
            .at_line(7);
        let text = issue.to_string();
        assert!(text.contains("a.json:7") && text.contains("[ID_X]") && text.contains("rules[2]"));
        assert_eq!(
            count_severity(
                &[issue.clone(), Issue::error(IssueCode::InvalidId, "x")],
                Severity::Error
            ),
            1
        );
        let json = serde_json::to_string(&issue).unwrap();
        assert!(json.contains("\"dangling_reference\""));
        assert_eq!(serde_json::from_str::<Issue>(&json).unwrap(), issue);
    }
}
