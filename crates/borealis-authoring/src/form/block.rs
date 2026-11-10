//! Rule blocks: one block per rule of the element, in order (spec 6).
//!
//! The block catalog (WB-05, module `blocks`) owns what a block's dialog fields are and its sentence. The form only
//! needs the [`BlockCodec`] interface: a block from a stored rule, and the rule a block writes. Until the catalog
//! lands, [`RawBlockCodec`] keeps every field the engine reads as stored and puts the rest (spec 3.4: `Rule::Unknown`,
//! select `default_behaviour`/`existing`/`stat`, stat `not_equipped`/`minimum`/`merge`, `extra`, custom-rule fields
//! the engine does not read) in the block's preserved bucket.

use super::preserved::{KeptPart, PreservedPart};
use super::{FormContext, FormProblem};
use borealis_content::rule_kinds;
use borealis_content::Rule;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// One rule block of a form (`HomebrewBlock`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// Stable key inside the draft (UI lists, undo); never written to content.
    pub key: String,
    /// The rule kind: `grant`, `select`, `stat`, `unknown`, or a custom kind (`resource`, `grant_spell`...).
    pub kind: String,
    /// The block as one plain sentence.
    #[serde(default)]
    pub sentence: String,
    /// The block dialog's values.
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    /// Parts of the rule the dialog does not edit, kept byte for byte.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved: Vec<PreservedPart>,
}

/// How rules become blocks and back (the block catalog implements it, WB-05).
pub trait BlockCodec {
    /// The block of a stored rule.
    fn read(&self, rule: &Rule, key: String, cx: &FormContext<'_>) -> Block;
    /// The rule a block writes; `Ok(None)` when it writes nothing (a kept rule whose part was removed). A problem's
    /// path is relative to the block (`fields.number`).
    ///
    /// # Errors
    /// A problem when the block's values cannot make a rule.
    fn write(&self, block: &Block, cx: &FormContext<'_>) -> Result<Option<Rule>, FormProblem>;
}

/// The interim codec: block fields are the rule's stored fields (lossless), the parts the engine does not read are
/// preserved.
#[derive(Debug, Clone, Copy, Default)]
pub struct RawBlockCodec;

/// The whole rule is kept (an unknown rule, or a custom kind the engine does not know).
const WHOLE: &str = "rule";

/// Fields of the three core kinds kept for round trip only (spec 3.4).
fn kept_core_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "select" => &["default_behaviour", "existing", "stat", "extra"],
        "stat" => &["not_equipped", "minimum", "merge", "extra"],
        "grant" => &["extra"],
        _ => &[],
    }
}

fn words(key: &str) -> String {
    key.replace(['_', '-'], " ")
}

fn kept(key: &str, label: String, value: Value) -> PreservedPart {
    PreservedPart {
        key: key.to_string(),
        label,
        kept: Some(KeptPart {
            location: key.to_string(),
            value,
            slot: None,
        }),
    }
}

/// Why a field of `kind` is kept, or `None` when the dialog edits it.
fn kept_reason(kind: &str, field: &str) -> Option<String> {
    if kept_core_fields(kind).contains(&field) {
        return Some(if field == "extra" {
            "Unknown settings of this rule".to_string()
        } else {
            format!("A setting Borealis does not use (“{}”)", words(field))
        });
    }
    let spec = rule_kinds::spec(kind)?;
    if spec.reads(field) {
        None
    } else if spec.knows(field) {
        Some(format!(
            "A setting Borealis keeps but does not apply (“{}”)",
            words(field)
        ))
    } else {
        Some(format!("An unknown setting (“{}”)", words(field)))
    }
}

fn is_core(kind: &str) -> bool {
    matches!(kind, "grant" | "select" | "stat")
}

impl BlockCodec for RawBlockCodec {
    fn read(&self, rule: &Rule, key: String, _cx: &FormContext<'_>) -> Block {
        let json = serde_json::to_value(rule).unwrap_or(Value::Null);
        let Value::Object(mut map) = json else {
            unreachable!("a rule serializes to an object")
        };
        let kind = match map.remove("kind") {
            Some(Value::String(k)) => k,
            _ => "unknown".to_string(),
        };
        let known = is_core(&kind) || rule_kinds::spec(&kind).is_some();
        if !known {
            let mut whole = map.clone();
            whole.insert("kind".into(), Value::String(kind.clone()));
            return Block {
                key,
                sentence: "Kept from the original: a rule Borealis does not read".into(),
                fields: BTreeMap::new(),
                preserved: vec![kept(
                    WHOLE,
                    format!("A rule Borealis does not read (“{}”)", words(&kind)),
                    Value::Object(whole),
                )],
                kind,
            };
        }
        let mut fields = BTreeMap::new();
        let mut preserved = Vec::new();
        for (k, v) in map {
            match kept_reason(&kind, &k) {
                Some(label) => preserved.push(kept(&k, label, v)),
                None => {
                    fields.insert(k, v);
                }
            }
        }
        Block {
            key,
            sentence: format!("Rule: {}", words(&kind)),
            kind,
            fields,
            preserved,
        }
    }

    fn write(&self, block: &Block, _cx: &FormContext<'_>) -> Result<Option<Rule>, FormProblem> {
        let mut map: Map<String, Value> = block.fields.clone().into_iter().collect();
        let mut whole = None;
        for p in &block.preserved {
            let Some(k) = &p.kept else { continue };
            if k.location == WHOLE {
                whole = Some(k.value.clone());
            } else {
                map.insert(k.location.clone(), k.value.clone());
            }
        }
        let json = match whole {
            Some(w) => w,
            None if !is_core(&block.kind) && rule_kinds::spec(&block.kind).is_none() => {
                return Ok(None)
            }
            None => {
                map.insert("kind".into(), Value::String(block.kind.clone()));
                Value::Object(map)
            }
        };
        serde_json::from_value(json).map(Some).map_err(|e| {
            FormProblem::new("fields", format!("This rule is not complete yet ({e})."))
        })
    }
}
