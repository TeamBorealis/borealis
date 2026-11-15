//! The embedded JSON schema and validation of raw JSON documents against it.

use crate::error::ContentError;
use serde_json::{json, Value};
use std::sync::OnceLock;

/// The content file schema (JSON Schema draft 2020-12) shipped with the crate.
pub const CONTENT_SCHEMA_JSON: &str = include_str!("../schema/content-format.schema.json");

/// The rules-text pack schema (`borealis-docs/1`, see `docs/rules-text-format.md`).
pub const DOCS_SCHEMA_JSON: &str = include_str!("../schema/docs-format.schema.json");

/// One schema violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaViolation {
    /// JSON pointer of the offending value.
    pub pointer: String,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for SchemaViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            if self.pointer.is_empty() {
                "/"
            } else {
                &self.pointer
            },
            self.message
        )
    }
}

/// Which document type to validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// A content file (`borealis-content/1`).
    Content,
    /// An index file (`borealis-index/1`; legacy `aurora-index/1` accepted).
    Index,
    /// A pack manifest (`borealis-pack/1`).
    Pack,
}

/// The parsed schema document.
///
/// # Errors
/// [`ContentError::Schema`] if the embedded schema is not valid JSON (a build defect).
pub fn schema_value() -> Result<&'static Value, ContentError> {
    static SCHEMA: OnceLock<Result<Value, String>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| serde_json::from_str(CONTENT_SCHEMA_JSON).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| ContentError::Schema(e.clone()))
}

fn validator(kind: DocumentKind) -> Result<&'static jsonschema::Validator, ContentError> {
    type Slot = OnceLock<Result<jsonschema::Validator, String>>;
    static CONTENT: Slot = OnceLock::new();
    static INDEX: Slot = OnceLock::new();
    static PACK: Slot = OnceLock::new();
    let (slot, def) = match kind {
        DocumentKind::Content => (&CONTENT, None),
        DocumentKind::Index => (&INDEX, Some("IndexFile")),
        DocumentKind::Pack => (&PACK, Some("PackManifest")),
    };
    let built = slot.get_or_init(|| {
        let schema = schema_value().map_err(|e| e.to_string())?;
        compile(schema, def)
    });
    built.as_ref().map_err(|e| ContentError::Schema(e.clone()))
}

/// A validator of `schema` (or of its `$defs` entry `def`) whose root only refers to it: the schema is a resource of
/// the validator, under its own `$id`. jsonschema checks every root it compiles against the draft's meta-schema with a
/// meta-schema validator it keeps for the life of the process, and checking the content schema grew that one by
/// about 134 MB (item `IzDXU`); a root that is only a `$ref` grows it by a few. What is checked and the errors are the
/// schema's own either way; the embedded schemas are checked against the meta-schema by a test.
fn compile(schema: &Value, def: Option<&str>) -> Result<jsonschema::Validator, String> {
    let id = schema["$id"]
        .as_str()
        .ok_or_else(|| "the embedded schema has no $id".to_string())?;
    let target = def.map_or_else(|| id.to_string(), |name| format!("{id}#/$defs/{name}"));
    let resource =
        jsonschema::Resource::from_contents(schema.clone()).map_err(|e| e.to_string())?;
    jsonschema::options()
        .with_resource(id, resource)
        .build(&json!({"$schema": schema["$schema"], "$ref": target}))
        .map_err(|e| e.to_string())
}

/// Validate a JSON document against the schema.
///
/// Returns the list of violations (empty when valid).
///
/// # Errors
/// [`ContentError::Schema`] if the schema itself cannot be compiled.
pub fn validate_document(
    kind: DocumentKind,
    doc: &Value,
) -> Result<Vec<SchemaViolation>, ContentError> {
    let v = validator(kind)?;
    Ok(v.iter_errors(doc)
        .map(|e| SchemaViolation {
            pointer: e.instance_path.to_string(),
            message: e.to_string(),
        })
        .collect())
}

/// Validate a content file document.
///
/// # Errors
/// [`ContentError::Schema`] if the schema itself cannot be compiled.
pub fn validate_content(doc: &Value) -> Result<Vec<SchemaViolation>, ContentError> {
    validate_document(DocumentKind::Content, doc)
}

/// Validate a rules-text pack document (`docs.json`, format `borealis-docs/1`).
///
/// Returns the list of violations (empty when valid).
///
/// # Errors
/// [`ContentError::Schema`] if the embedded schema is not valid JSON or cannot be compiled (a build defect).
pub fn validate_docs(doc: &Value) -> Result<Vec<SchemaViolation>, ContentError> {
    static DOCS: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();
    let built = DOCS.get_or_init(|| {
        let schema: Value = serde_json::from_str(DOCS_SCHEMA_JSON).map_err(|e| e.to_string())?;
        compile(&schema, None)
    });
    let v = built
        .as_ref()
        .map_err(|e| ContentError::Schema(e.clone()))?;
    Ok(v.iter_errors(doc)
        .map(|e| SchemaViolation {
            pointer: e.instance_path.to_string(),
            message: e.to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The validators compile the embedded schemas as resources, so jsonschema no longer checks them against the
    /// meta-schema when it builds them: this test does.
    #[test]
    fn the_embedded_schemas_are_valid_against_the_meta_schema() {
        for text in [CONTENT_SCHEMA_JSON, DOCS_SCHEMA_JSON] {
            let schema: Value = serde_json::from_str(text).unwrap();
            assert_eq!(
                schema["$schema"],
                "https://json-schema.org/draft/2020-12/schema"
            );
            assert!(schema["$id"].is_string());
            if let Err(e) = jsonschema::meta::validate(&schema) {
                panic!("{}: {e}", schema["$id"]);
            }
        }
    }

    /// The validator as it was built before `IzDXU`: the schema itself as the root (an `$defs` entry: a root holding a
    /// copy of the `$defs`).
    fn compiled_as_the_root(schema: &Value, def: Option<&str>) -> jsonschema::Validator {
        let root = match def {
            None => schema.clone(),
            Some(name) => json!({
                "$schema": schema["$schema"],
                "$ref": format!("#/$defs/{name}"),
                "$defs": schema["$defs"],
            }),
        };
        jsonschema::validator_for(&root).unwrap()
    }

    fn errors(v: &jsonschema::Validator, doc: &Value) -> Vec<(String, String)> {
        v.iter_errors(doc)
            .map(|e| (e.instance_path.to_string(), e.to_string()))
            .collect()
    }

    /// Same validation, same errors in the same order, for documents with problems at every level.
    #[test]
    fn a_schema_compiled_as_a_resource_reports_what_it_reported_as_the_root() {
        let content: Value = serde_json::from_str(CONTENT_SCHEMA_JSON).unwrap();
        let docs: Value = serde_json::from_str(DOCS_SCHEMA_JSON).unwrap();
        let el = json!({"id":"ID_A","name":"A","type":"Spell","source":"S","rules":[
            {"kind":"grant","type":"Spell","id":"ID_B"},
            {"kind":"select","type":"Spell","name":"Pick","supports":"Wizard","number":2},
            {"kind":"stat","name":"ac","value":"1"}],
            "setters":{"level":1},"spellcasting":{"name":"Wizard","list":[{"supports":"Wizard"}]}});
        let mut cases = vec![
            json!({"format":"borealis-content/1","elements":[el.clone()]}),
            json!({"format":"borealis-content/1","elements":[]}),
            json!({"format":"x","elements":7}),
            json!({"elements":[{"id":"","name":3,"type":"T","source":"Core","bogus":true}]}),
            json!([]),
            json!(null),
        ];
        for (k, bad) in [
            ("rules", json!([{"kind":"grant","idd":"x"}])),
            ("rules", json!([{"kind":"select","type":7,"number":"two"}])),
            ("rules", json!("grant")),
            ("setters", json!([1])),
            ("spellcasting", json!({"name":3,"list":"Wizard"})),
            ("id", json!(null)),
            ("unknown_field", json!({})),
        ] {
            let mut e = el.clone();
            e[k] = bad;
            cases.push(json!({"format":"borealis-content/1","elements":[e]}));
        }
        let index = vec![
            json!({"format":"borealis-index/1","info":{"name":"x"},"files":[]}),
            json!({"format":"aurora-index/1","info":{},"files":[{"name":7}]}),
            json!({"format":"borealis-index/1","files":"a"}),
        ];
        let pack = vec![
            json!({"format":"borealis-pack/1","id":"p","name":"P","version":"1","rulesets":["dnd2014"],"files":[]}),
            json!({"format":"x"}),
            json!({"format":"borealis-pack/1","id":1,"rulesets":"dnd2014","files":[{}]}),
        ];
        let sha = "0".repeat(64);
        let rules_text = vec![
            json!({"format":"borealis-docs/1","ruleset":"dnd2024",
                "info":{"id":"p","name":"P","version":"1.0.0","license":"CC-BY-4.0","attribution":"a","description":"d",
                        "source":{"document":"SRD","pdf":"x.pdf","pdf_sha256":sha}},
                "sections":[{"id":"combat/cover","path":[],"page":0,"html":"<p>x</p>"}]}),
            json!({"format":"borealis-docs/2","sections":[{}]}),
        ];
        let mut checked = 0;
        for (schema, def, docs) in [
            (&content, None, &cases),
            (&content, Some("IndexFile"), &index),
            (&content, Some("PackManifest"), &pack),
            (&content, Some("IndexFile"), &cases),
            (&docs, None, &rules_text),
        ] {
            let (old, new) = (
                compiled_as_the_root(schema, def),
                compile(schema, def).unwrap(),
            );
            for doc in docs {
                let expected = errors(&old, doc);
                checked += expected.len();
                assert_eq!(errors(&new, doc), expected, "{def:?}: {doc}");
            }
        }
        assert!(checked > 30, "{checked} errors compared");
    }

    #[test]
    fn minimal_documents_validate() {
        let ok = json!({"format":"borealis-content/1","elements":[
            {"id":"ID_A","name":"A","type":"Anything Goes","source":"Core"}]});
        assert!(validate_content(&ok).unwrap().is_empty());
    }

    #[test]
    fn docs_schema_accepts_a_section_and_rejects_missing_page_or_path() {
        let sha = "0".repeat(64);
        let mut doc = json!({"format":"borealis-docs/1","ruleset":"dnd2024",
            "info":{"id":"p","name":"P","version":"1.0.0","license":"CC-BY-4.0","attribution":"a","description":"d",
                    "source":{"document":"SRD","pdf":"x.pdf","pdf_sha256":sha}},
            "sections":[{"id":"combat/cover","path":["Combat","Cover"],"page":15,"html":"<p>x</p>"}]});
        assert!(validate_docs(&doc).unwrap().is_empty());
        doc["sections"][0]["page"] = json!(0);
        doc["sections"][0]["path"] = json!([]);
        let v = validate_docs(&doc).unwrap();
        assert!(v.iter().any(|x| x.pointer == "/sections/0/page"));
        assert!(v.iter().any(|x| x.pointer == "/sections/0/path"));
        assert!(!validate_docs(&json!({"format":"borealis-docs/2"}))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn legacy_format_id_still_validates() {
        let old = json!({"format":"aurora-content/1","elements":[
            {"id":"ID_A","name":"A","type":"Anything Goes","source":"Core"}]});
        assert!(validate_content(&old).unwrap().is_empty());
    }

    #[test]
    fn violations_are_located() {
        let bad = json!({"format":"borealis-content/1","elements":[
            {"id":"","name":"A","type":"T","source":"Core","bogus":true}]});
        let v = validate_content(&bad).unwrap();
        assert!(!v.is_empty());
        assert!(v.iter().any(|x| x.pointer.starts_with("/elements/0")));
        assert!(!validate_content(&json!({"elements":[]}))
            .unwrap()
            .is_empty());
        assert!(v[0].to_string().contains(':'));
    }

    #[test]
    fn index_and_pack_schemas() {
        let new_idx = json!({"format":"borealis-index/1","info":{"name":"x"},"files":[]});
        assert!(validate_document(DocumentKind::Index, &new_idx)
            .unwrap()
            .is_empty());
        let idx = json!({"format":"aurora-index/1","info":{"name":"x"},"files":[{"name":"a.xml","url":"https://e.invalid/a.xml"}]});
        assert!(validate_document(DocumentKind::Index, &idx)
            .unwrap()
            .is_empty());
        let pack = json!({"format":"borealis-pack/1","id":"p","name":"P","version":"1","rulesets":["dnd2014"],"files":[]});
        assert!(validate_document(DocumentKind::Pack, &pack)
            .unwrap()
            .is_empty());
        assert!(
            !validate_document(DocumentKind::Pack, &json!({"format":"x"}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn custom_rules_validate_but_misspelled_known_kinds_do_not() {
        let mut doc = json!({"format":"borealis-content/1","elements":[
            {"id":"ID_A","name":"A","type":"T","source":"S","rules":[{"kind":"select.size","options":["Small"]}]}]});
        assert!(validate_content(&doc).unwrap().is_empty());
        doc["elements"][0]["rules"] = json!([{"kind":"grant","idd":"x"}]);
        assert!(!validate_content(&doc).unwrap().is_empty());
    }
}
