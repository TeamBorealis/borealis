//! Validation shared by import review edits and homebrew: schema, store load, engine check and
//! the homebrew balance flags, all without a model call.

use borealis_ai::task::{ContentValidator, JsonSchemaValidator};
use borealis_assist::balance::CharacterCheck;
use borealis_assist::{balance_report, load_into_engine, BalanceReport, Baselines};
use borealis_content::{ContentFile, ContentStoreBuilder, Ruleset};
use serde_json::{json, Value};

/// What a check found.
#[derive(Debug, Clone, Default)]
pub struct Check {
    /// Problems that stop the content from being used.
    pub errors: Vec<String>,
    /// Balance flags and notes (informational).
    pub warnings: Vec<String>,
    /// One report per element (empty when the document did not load).
    pub reports: Vec<BalanceReport>,
}

/// A content document holding `elements`.
#[must_use]
pub fn content_doc(elements: &[Value]) -> Value {
    json!({ "format": borealis_content::file::ContentFormat::ID, "elements": elements })
}

/// Human-readable lines for the flagged metrics and notes of a report.
#[must_use]
pub fn balance_lines(report: &BalanceReport) -> Vec<String> {
    report
        .flagged()
        .map(|f| {
            format!(
                "{}: {} (SRD {}-{}, median {}) {:?}. {}",
                f.metric,
                f.value,
                f.baseline_range[0],
                f.baseline_range[1],
                f.baseline_median,
                f.severity,
                f.note
            )
        })
        .chain(report.notes.iter().cloned())
        .collect()
}

/// Check a content document on top of `base`.
#[must_use]
pub fn check_content(base: &ContentStoreBuilder, ruleset: Ruleset, doc: &Value) -> Check {
    let mut out = Check::default();
    let schema = JsonSchemaValidator::content_format().validate(doc);
    if !schema.is_empty() {
        out.errors = schema.iter().map(ToString::to_string).collect();
        return out;
    }
    let engine = load_into_engine(base, ruleset, doc);
    out.errors.extend(engine.dangling_refs.iter().cloned());
    out.errors.extend(engine.store_errors.iter().cloned());
    for (id, c) in &engine.characters {
        if let CharacterCheck::Refused(why) = c {
            out.errors.push(format!("{id}: {why}"));
        }
    }
    if let Ok(file) = ContentFile::from_json_str(&doc.to_string()) {
        let baselines = Baselines::from_store(&base.clone().build(), ruleset);
        out.reports = file
            .elements
            .iter()
            .map(|e| balance_report(&baselines, e))
            .collect();
        out.warnings = out.reports.iter().flat_map(balance_lines).collect();
    }
    out
}
