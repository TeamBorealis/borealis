//! Rule blocks: one block per rule of the element, in order (spec 6).
//!
//! The block catalog (WB-05, module [`crate::blocks`]) owns what a block's dialog fields are and its sentence; the
//! form talks to it through the [`BlockCodec`] interface: a block from a stored rule, the rule a block writes, the
//! block's sentence after an edit, and the problems of its builder values. [`super::CatalogBlockCodec`] is the codec.

use super::preserved::{KeptPart, PreservedPart};
use super::{FormContext, FormProblem};
use borealis_content::Rule;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// One rule block of a form (`HomebrewBlock`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// Stable key inside the draft (UI lists, undo); never written to content.
    pub key: String,
    /// The block kind: `grant`, `select`, `list` (a select of type `List`), `stat`, a custom kind (`resource`,
    /// `grant_spell`...), `speed_multiply` inside a toggle's effects, or the kind of a rule kept whole (`unknown`).
    pub kind: String,
    /// The block as one plain sentence.
    #[serde(default)]
    pub sentence: String,
    /// The block dialog's values (shapes: [`super::block_codec`]).
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    /// Parts of the rule the dialog does not edit, kept byte for byte.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved: Vec<PreservedPart>,
    /// Backend-only: the fields as read and their stored JSON; absent on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memo: Option<BlockMemo>,
}

/// What the backend remembers about a block read from a stored rule (the stored-text rule: a field whose value is
/// unchanged writes its stored JSON back).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockMemo {
    /// The block kind it was read as (a block whose kind changed writes canonically).
    pub kind: String,
    /// Each field's value as read.
    #[serde(default)]
    pub read: BTreeMap<String, Value>,
    /// Each field's stored JSON.
    #[serde(default)]
    pub raw: BTreeMap<String, Value>,
}

/// How rules become blocks and back (implemented by [`super::CatalogBlockCodec`]).
pub trait BlockCodec {
    /// The block of a stored rule.
    fn read(&self, rule: &Rule, key: String, cx: &FormContext<'_>) -> Block;
    /// The rule a block writes; `Ok(None)` when it writes nothing (a kept rule whose part was removed). A problem's
    /// path is relative to the block (`fields.number`).
    ///
    /// # Errors
    /// A problem when the block's values cannot make a rule.
    fn write(&self, block: &Block, cx: &FormContext<'_>) -> Result<Option<Rule>, FormProblem>;
    /// The block with its sentence (and its nested blocks' sentences) written from its current values.
    fn refresh(&self, block: &Block, _cx: &FormContext<'_>) -> Block {
        block.clone()
    }
    /// The problems of the block's builder values (conditions, tag filters, formulas), paths relative to the block.
    fn check(&self, _block: &Block, _cx: &FormContext<'_>) -> Vec<FormProblem> {
        Vec::new()
    }
}

/// The location of a preserved part holding a whole rule (an unknown rule, or a kind the catalog does not know).
pub(crate) const WHOLE: &str = "rule";

/// A preserved part of a rule: the field `key` with its stored value.
pub(crate) fn kept_part(key: &str, label: String, value: Value) -> PreservedPart {
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
