//! Loading submitted content in memory on top of the installed packs. Nothing is written.

use std::time::{Duration, Instant};

use borealis_content::file::PackOrigin;
use borealis_content::{ContentFile, ContentStore, Issue, Severity};
use serde_json::{json, Value};

use super::Tools;

/// Maximum issues listed in one result.
pub const MAX_ISSUES: usize = 30;

const FORMAT: &str = borealis_content::file::ContentFormat::ID;

/// Largest model-supplied bundle accepted (serialized JSON bytes).
pub const MAX_BUNDLE_BYTES: usize = 1024 * 1024;
/// Most elements in one model-supplied bundle.
pub const MAX_BUNDLE_ELEMENTS: usize = 2_000;
/// Wall-clock budget for one `validate_element` / `class_sweep` call.
pub const CALL_BUDGET: Duration = Duration::from_secs(20);

/// Cooperative wall-clock deadline, checked between the engine's steps.
pub struct Budget(Instant);

impl Budget {
    pub fn start() -> Self {
        Self(Instant::now())
    }

    pub fn check(&self) -> Result<(), String> {
        if self.0.elapsed() > CALL_BUDGET {
            Err(format!(
                "time budget of {}s exceeded; submit a smaller bundle",
                CALL_BUDGET.as_secs()
            ))
        } else {
            Ok(())
        }
    }
}

/// Stable identity of an issue, for "new compared with the base content" diffing.
pub fn fingerprint(issue: &Issue) -> String {
    format!(
        "{:?}|{}|{:?}|{:?}",
        issue.code, issue.message, issue.element, issue.path
    )
}

/// Turn the accepted input shapes into a content-file document.
pub fn normalize(input: &Value) -> Result<Value, String> {
    let parsed;
    let value = if let Some(text) = input.as_str() {
        if text.len() > MAX_BUNDLE_BYTES {
            return Err(too_big());
        }
        parsed = serde_json::from_str::<Value>(text)
            .map_err(|e| format!("the JSON string does not parse: {e}"))?;
        &parsed
    } else {
        input
    };
    let mut doc = match value {
        Value::Array(items) => json!({"elements": items}),
        Value::Object(obj) if obj.contains_key("elements") => value.clone(),
        Value::Object(obj) if obj.contains_key("id") => json!({"elements": [value]}),
        _ => {
            return Err("expected a content file {elements: [...]}, an element or an array".into())
        }
    };
    let count = doc["elements"].as_array().map_or(0, Vec::len);
    if count > MAX_BUNDLE_ELEMENTS
        || serde_json::to_vec(&doc).map_or(0, |v| v.len()) > MAX_BUNDLE_BYTES
    {
        return Err(too_big());
    }
    if let Some(obj) = doc.as_object_mut() {
        obj.entry("format").or_insert_with(|| json!(FORMAT));
    }
    Ok(doc)
}

fn too_big() -> String {
    format!(
        "bundle too large: at most {MAX_BUNDLE_ELEMENTS} elements and {} KiB of JSON",
        MAX_BUNDLE_BYTES / 1024
    )
}

/// The base content with `doc` loaded on top as user content (a repeated id replaces the original).
///
/// This clones the base builder (all loaded pack files) once per call: `ContentStoreBuilder::build`
/// consumes the builder and the store has no borrowing overlay, so a clone is the cheapest
/// available path. Call volume is bounded by the bundle caps and the per-call time budget.
pub fn build(tools: &Tools, doc: &Value) -> Result<ContentStore, String> {
    let file: ContentFile = serde_json::from_value(doc.clone())
        .map_err(|e| format!("not loadable as a content file: {e}"))?;
    Ok(tools
        .builder
        .clone()
        .add_file("overlay.json", PackOrigin::Custom, file)
        .build())
}

/// Issues of `store` that the unmodified base content does not already have.
pub fn new_issues(tools: &Tools, store: &ContentStore) -> Vec<Issue> {
    let base = tools.baseline();
    store
        .validate()
        .into_iter()
        .chain(store.diagnostics().iter().cloned())
        .filter(|i| !base.contains(&fingerprint(i)))
        .collect()
}

/// Counts and a capped list, errors first.
pub fn summarize(mut issues: Vec<Issue>) -> Value {
    issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
    let count = |s: Severity| issues.iter().filter(|i| i.severity == s).count();
    json!({
        "errors": count(Severity::Error),
        "warnings": count(Severity::Warning),
        "info": count(Severity::Info),
        "issues": issues.iter().take(MAX_ISSUES).collect::<Vec<_>>(),
        "truncated": issues.len() > MAX_ISSUES,
    })
}
