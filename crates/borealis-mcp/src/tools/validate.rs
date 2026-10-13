//! `validate_element`.

use borealis_ai::task::ContentValidator;
use borealis_content::{IssueCode, Severity};
use serde::Deserialize;
use serde_json::{json, Value};

use super::overlay::{build, new_issues, normalize, summarize, Budget};
use super::{parse_args, parse_ruleset, Tools};

const MAX_SCHEMA_ISSUES: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    json: Option<Value>,
    elements: Option<Vec<Value>>,
    /// Edition of the content (`2014`, `2024`, `dnd2014`, `dnd2024`, `any`); the content-format `ruleset`.
    ruleset: Option<String>,
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let budget = Budget::start();
    let a: Args = parse_args(args)?;
    let input = match (a.json, a.elements) {
        (Some(v), None) => v,
        (None, Some(list)) => Value::Array(list),
        _ => return Err("pass exactly one of `json` or `elements`".into()),
    };
    let mut doc = normalize(&input)?;
    let ruleset = a.ruleset.as_deref().map(parse_ruleset).transpose()?;
    if let Some(r) = ruleset {
        apply_ruleset(&mut doc, r.as_str());
    }
    normalize_element_rulesets(&mut doc);
    let schema: Vec<Value> = tools
        .schema()
        .validate(&doc)
        .into_iter()
        .take(MAX_SCHEMA_ISSUES)
        .map(|i| json!({"path": i.path, "message": i.message}))
        .collect();
    if !schema.is_empty() {
        return Ok(json!({"ok": false, "stage": "schema", "schema_issues": schema}));
    }
    budget.check()?;
    let store = build(tools, &doc)?;
    budget.check()?;
    let issues = new_issues(tools, &store);
    budget.check()?;
    // A dangling reference is only a warning in the store, but generated content must not ship one.
    let ok = !issues
        .iter()
        .any(|i| i.severity == Severity::Error || i.code == IssueCode::DanglingReference);
    let summary = summarize(issues);
    Ok(json!({"ok": ok, "stage": "store", "report": summary}))
}

/// Record the tool's `ruleset` as the file-level ruleset unless the content already names one.
fn apply_ruleset(doc: &mut Value, ruleset: &str) {
    if let Some(obj) = doc.as_object_mut() {
        obj.entry("ruleset").or_insert_with(|| json!(ruleset));
    }
}

/// Models write `"2024"`; the schema's `Ruleset` tag is `dnd2024`. Fix the short form per element and on the file.
fn normalize_element_rulesets(doc: &mut Value) {
    let fix = |v: &mut Value| {
        let Some(text) = v.as_str() else { return };
        if let Ok(r) = parse_ruleset(text) {
            *v = json!(r.as_str());
        }
    };
    if let Some(v) = doc.get_mut("ruleset") {
        fix(v);
    }
    for el in doc["elements"].as_array_mut().into_iter().flatten() {
        if let Some(v) = el.get_mut("ruleset") {
            fix(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_is_applied_and_short_forms_normalised() {
        let mut doc = json!({"elements": [{"id": "A", "ruleset": "2024"}, {"id": "B", "ruleset": "dnd2014"}]});
        apply_ruleset(&mut doc, "dnd2024");
        normalize_element_rulesets(&mut doc);
        assert_eq!(doc["ruleset"], "dnd2024");
        assert_eq!(doc["elements"][0]["ruleset"], "dnd2024");
        assert_eq!(doc["elements"][1]["ruleset"], "dnd2014");
        let mut named = json!({"ruleset": "dnd2014", "elements": []});
        apply_ruleset(&mut named, "dnd2024");
        assert_eq!(named["ruleset"], "dnd2014");
    }
}
