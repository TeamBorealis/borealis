//! Validation of generated content.
//!
//! [`ContentValidator`] is the seam `borealis-content` will implement with its real loader checks.
//! Until then [`JsonSchemaValidator`] validates against `content-format.schema.json`.

use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::AiError;

/// The content-pack JSON Schema, kept identical to `borealis-content`'s `schema/content-format.schema.json`.
pub const CONTENT_FORMAT_SCHEMA: &str = include_str!("../../schemas/content-format.schema.json");

/// Maximum issues fed back to the model in one repair round (keeps prompts small).
pub const MAX_ISSUES_REPORTED: usize = 20;

/// One problem found in a content document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// JSON pointer-ish path (`/elements/0/id`), empty for the root.
    pub path: String,
    /// Human-readable description.
    pub message: String,
}

impl ValidationIssue {
    /// Convenience constructor.
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Validates a whole content-pack document (`{"format": "...", "elements": [...]}`).
pub trait ContentValidator: Send + Sync {
    /// Return every issue found; empty means valid.
    fn validate(&self, content: &Value) -> Vec<ValidationIssue>;
}

/// Default [`ContentValidator`] backed by a JSON Schema.
pub struct JsonSchemaValidator {
    validator: Validator,
}

impl std::fmt::Debug for JsonSchemaValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JsonSchemaValidator")
    }
}

impl JsonSchemaValidator {
    /// Validator for the bundled content-format schema.
    pub fn content_format() -> Self {
        let schema: Value =
            serde_json::from_str(CONTENT_FORMAT_SCHEMA).expect("bundled schema is valid JSON");
        Self::from_schema(&schema).expect("bundled schema compiles")
    }

    /// Validator for an arbitrary schema.
    pub fn from_schema(schema: &Value) -> Result<Self, AiError> {
        let validator = jsonschema::validator_for(schema)
            .map_err(|e| AiError::Invalid(format!("schema does not compile: {e}")))?;
        Ok(Self { validator })
    }
}

impl ContentValidator for JsonSchemaValidator {
    fn validate(&self, content: &Value) -> Vec<ValidationIssue> {
        self.validator
            .iter_errors(content)
            .map(|e| ValidationIssue::new(e.instance_path.to_string(), e.to_string()))
            .collect()
    }
}

/// Render issues as bullet strings for repair prompts, capped at [`MAX_ISSUES_REPORTED`].
pub fn issues_to_strings(issues: &[ValidationIssue]) -> Vec<String> {
    let mut v: Vec<String> = issues
        .iter()
        .take(MAX_ISSUES_REPORTED)
        .map(ToString::to_string)
        .collect();
    if issues.len() > MAX_ISSUES_REPORTED {
        v.push(format!(
            "...and {} more issue(s)",
            issues.len() - MAX_ISSUES_REPORTED
        ));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn good() -> Value {
        json!({
            "format": "borealis-content/1",
            "elements": [{"id": "ID_HB_FEAT_TEST", "name": "Test", "type": "Feat", "source": "Homebrew"}]
        })
    }

    #[test]
    fn accepts_minimal_valid_pack() {
        let v = JsonSchemaValidator::content_format();
        assert!(v.validate(&good()).is_empty());
    }

    #[test]
    fn reports_missing_required_and_unknown_fields_with_paths() {
        let v = JsonSchemaValidator::content_format();
        let bad = json!({
            "format": "borealis-content/1",
            "elements": [{"id": "ID_X", "name": "X", "type": "Feat", "bogus": 1}]
        });
        let issues = v.validate(&bad);
        assert!(
            issues
                .iter()
                .any(|i| i.path == "/elements/0" && i.message.contains("source")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.message.contains("bogus")),
            "{issues:?}"
        );
    }

    #[test]
    fn accepts_legacy_format_marker() {
        let v = JsonSchemaValidator::content_format();
        let mut legacy = good();
        legacy["format"] = json!("aurora-content/1");
        assert!(v.validate(&legacy).is_empty());
    }

    #[test]
    fn rejects_wrong_format_marker() {
        let v = JsonSchemaValidator::content_format();
        let issues = v.validate(&json!({"format": "other", "elements": []}));
        assert!(!issues.is_empty());
    }

    #[test]
    fn bad_schema_is_an_error() {
        let err = JsonSchemaValidator::from_schema(&json!({"type": 12})).unwrap_err();
        assert!(matches!(err, AiError::Invalid(_)));
    }

    #[test]
    fn issue_list_is_capped_for_prompts() {
        let issues: Vec<_> = (0..30)
            .map(|i| ValidationIssue::new(format!("/{i}"), "bad"))
            .collect();
        let s = issues_to_strings(&issues);
        assert_eq!(s.len(), MAX_ISSUES_REPORTED + 1);
        assert!(s.last().unwrap().contains("10 more"));
    }

    #[test]
    fn display_omits_empty_path() {
        assert_eq!(ValidationIssue::new("", "oops").to_string(), "oops");
        assert_eq!(ValidationIssue::new("/a", "oops").to_string(), "/a: oops");
    }
}
