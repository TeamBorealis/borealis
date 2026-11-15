//! The content schema's validators stay small for the life of the process (item `IzDXU`).
//!
//! jsonschema checks the root of every validator it builds against the draft 2020-12 meta-schema, with a meta-schema
//! validator it keeps for the life of the process. Checking the content schema itself (57 KB, 70 `$ref`s) grew that
//! validator by about 134 MB of heap, which a phone never got back. The resident size of this test's own process is
//! read before and after the first validation of each kind (the only test in this binary, so nothing else allocates).

#![cfg(unix)]

use borealis_content::schema::{validate_content, validate_document, DocumentKind};
use serde_json::json;

/// What building the three content validators may add to the process: the old path added about 136 MB.
const LIMIT_KB: usize = 40 * 1024;

fn rss_kb() -> usize {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8(out.stdout)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .expect("ps prints the resident size in KB")
}

#[test]
fn the_schema_validators_hold_a_few_megabytes_not_a_hundred() {
    let content = json!({"format": "borealis-content/1", "elements": [
        {"id": "ID_A", "name": "A", "type": "Spell", "source": "S", "rules": [{"kind": "grant", "type": "Spell", "id": "ID_B"}]}]});
    let index = json!({"format": "borealis-index/1", "info": {"name": "x"}, "files": []});
    let pack = json!({"format": "borealis-pack/1", "id": "p", "name": "P", "version": "1", "rulesets": ["dnd2014"], "files": []});
    let before = rss_kb();
    assert!(validate_content(&content).unwrap().is_empty());
    assert!(validate_document(DocumentKind::Index, &index)
        .unwrap()
        .is_empty());
    assert!(validate_document(DocumentKind::Pack, &pack)
        .unwrap()
        .is_empty());
    let grown = rss_kb().saturating_sub(before);
    assert!(
        grown < LIMIT_KB,
        "the schema validators added {} MB to the process",
        grown / 1024
    );
}
