//! Shared rich-text fixtures (`tests/fixtures/rich_text.json`): the UI's schema mirror
//! (`ui/src/pages/homebrew/rich/rich.test.ts`) runs the same file, so both sides agree on the tree's JSON, the
//! normalization, the escape lint and on what pasted content becomes.

use borealis_content::rich::{
    fix, fix_text, lint, lint_text, normalize_plain_text, parse, render, Doc,
};
use serde_json::{json, Value};

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/rich_text.json")).unwrap()
}

fn cases<'a>(all: &'a Value, key: &str) -> &'a Vec<Value> {
    let list = all[key]
        .as_array()
        .unwrap_or_else(|| panic!("fixtures lack {key}"));
    assert!(!list.is_empty(), "no {key} fixtures");
    list
}

fn doc(value: &Value) -> Doc {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn round_trip_cases() {
    let all = fixtures();
    for case in cases(&all, "roundtrip") {
        let name = &case["name"];
        let html = case["html"].as_str().unwrap();
        let parsed = parse(html);
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            case["doc"],
            "{name}: parse"
        );
        assert_eq!(render(&doc(&case["doc"])), html, "{name}: render");
    }
}

#[test]
fn normalize_cases() {
    let all = fixtures();
    for case in cases(&all, "normalize") {
        let normalized = doc(&case["doc"]).normalized();
        assert_eq!(
            serde_json::to_value(&normalized).unwrap(),
            case["normalized"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn lint_cases() {
    let all = fixtures();
    for case in cases(&all, "lint") {
        let text = case["text"].as_str().unwrap();
        let findings: Vec<Value> = lint_text(text)
            .into_iter()
            .map(|f| json!({"kind": f.kind, "found": f.found, "replacement": f.replacement}))
            .collect();
        assert_eq!(Value::Array(findings), case["findings"], "{text:?}");
        assert_eq!(fix_text(text), case["fixed"].as_str().unwrap(), "{text:?}");
    }
    for case in cases(&all, "plain") {
        let text = case["text"].as_str().unwrap();
        assert_eq!(
            normalize_plain_text(text),
            case["normalized"].as_str().unwrap(),
            "{text:?}"
        );
    }
}

#[test]
fn doc_lint_cases() {
    let all = fixtures();
    for case in cases(&all, "doc_lint") {
        let name = &case["name"];
        let d = doc(&case["doc"]);
        assert_eq!(
            serde_json::to_value(lint(&d)).unwrap(),
            case["findings"],
            "{name}"
        );
        let fixed = fix(&d);
        assert_eq!(
            serde_json::to_value(&fixed).unwrap(),
            case["fixed"],
            "{name}"
        );
        assert_eq!(
            render(&fixed),
            case["fixed_html"].as_str().unwrap(),
            "{name}"
        );
    }
}

/// What the UI's paste rules produce is canonical for the writer: normalized, clean, and stable through HTML.
#[test]
fn paste_cases() {
    let all = fixtures();
    for case in cases(&all, "paste") {
        let name = &case["name"];
        let d = doc(&case["doc"]);
        let html = render(&d);
        assert_eq!(html, case["rendered"].as_str().unwrap(), "{name}");
        assert_eq!(parse(&html), d, "{name}: parse(render)");
        assert_eq!(d.normalized(), d, "{name}: normalized");
        assert!(lint(&d).is_empty(), "{name}: escape artefacts left");
        assert_eq!(d.kept_count(), 0, "{name}: paste never keeps");
    }
}
