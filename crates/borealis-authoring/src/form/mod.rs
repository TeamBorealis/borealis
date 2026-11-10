//! The creator's form model (spec 3, 4, 15.1 `form`): one [`FormState`] per draft element, read from content with
//! [`element_to_form`] and written back with [`form_to_elements`].
//!
//! * **Fields** ([`kinds`]): each form field is bound to every model location that holds it (spec 3.3), for
//!   example the setter `script` of a Language, or the setters, tags and `data.rarity` of its rarity.
//! * **Blocks** ([`block`]): one block per rule, in order, each with its plain sentence. The block catalog (WB-05,
//!   [`crate::blocks`]) gives them their dialog fields through [`BlockCodec`] ([`CatalogBlockCodec`]); fields a block
//!   doesn't edit go to the block's own preserved bucket.
//! * **Preserved bucket** ([`preserved`]): every part of the element no field edits (`extra`, unbound `data` keys,
//!   `profile_overrides`...), written back byte for byte (spec 3.4). The UI lists them ("Kept from the original")
//!   and can only remove them.
//! * **Children**: elements edited inside this one (a class's features), each its own form.
//!
//! The identity rule (spec 18.2): `form_to_elements(element_to_form(e)) == [e]`, byte for byte. A field writes the
//! stored bytes of its locations back while its value is unchanged ([`Memo`]); an edited field writes every location
//! it is bound to in canonical form, and only the 2024 mirror locations the element already had.
//!
//! Deterministic, no AI (G7).

mod append;
pub mod block;
mod block_codec;
mod codec;
pub mod descriptor;
pub mod kinds;
mod paths;
pub mod preserved;
mod read;
mod ui;
pub mod value;
mod write;

pub use append::{append_to_form, form_to_append, APPEND_KIND};
pub use block::{Block, BlockCodec, BlockMemo};
pub use block_codec::CatalogBlockCodec;
pub(crate) use block_codec::INCOMPLETE;
pub use descriptor::descriptors;
pub use kinds::{
    append_kind, kind, kind_for_type, kinds, Control, FieldDef, KindDef, KindGroup, OTHER_KIND,
};
pub use paths::{check_fields, form_path_for_issue, form_path_for_location};
pub use preserved::{KeptPart, PreservedPart};
pub use read::{element_group_to_form, element_to_form};
pub use ui::{assign_ids, merge_ui, refresh_blocks, to_ui};
pub use write::{form_to_element, form_to_elements};

use crate::expr_build::condition::ConditionContext;
use borealis_content::ContentStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// The editable state of one draft element (`HomebrewForm` on the wire, spec 15.1 `FormState`).
///
/// Serialized, it is the draft file's form (WB-06). [`to_ui`] strips what only the backend needs (the [`Memo`] and
/// the preserved bytes); [`merge_ui`] puts them back on the form the UI sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormState {
    /// Stable key of this form inside the draft (children are matched by it); never written to content.
    #[serde(default)]
    pub key: String,
    /// The creator form (`language`, `other`...), [`KindDef::id`].
    pub kind: String,
    /// One value per [`FieldDef::key`] of the kind (shapes: [`value`]).
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    /// The rule blocks, one per rule, in order.
    #[serde(default)]
    pub blocks: Vec<Block>,
    /// Child elements edited inside this one.
    #[serde(default)]
    pub children: Vec<FormState>,
    /// Parts kept from the original (read-only, removable).
    #[serde(default)]
    pub preserved: Vec<PreservedPart>,
    /// Backend-only state (the id, the stored bytes of each field); absent on the wire.
    #[serde(default, skip_serializing_if = "Memo::is_empty")]
    pub memo: Memo,
}

impl FormState {
    /// An empty form of `kind`, for a new element (no id yet).
    #[must_use]
    pub fn new(key: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: kind.into(),
            fields: BTreeMap::new(),
            blocks: Vec::new(),
            children: Vec::new(),
            preserved: Vec::new(),
            memo: Memo::default(),
        }
    }

    /// The element id, once the element has one (frozen on its first save, spec 5.3).
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.memo.id.as_deref()
    }

    /// A copy with `field` set to `value` (forms are values: every edit makes a new state).
    #[must_use]
    pub fn with_field(&self, field: &str, value: Value) -> Self {
        let mut next = self.clone();
        next.fields.insert(field.to_string(), value);
        next
    }

    /// A copy without the preserved part `key` (the "Remove" action of "Kept from the original").
    #[must_use]
    pub fn without_preserved(&self, key: &str) -> Self {
        let mut next = self.clone();
        next.preserved.retain(|p| p.key != key);
        next
    }

    /// This form and every descendant, depth first, parent before children.
    #[must_use]
    pub fn flatten(&self) -> Vec<&FormState> {
        let mut out = vec![self];
        for c in &self.children {
            out.extend(c.flatten());
        }
        out
    }
}

/// What the backend remembers about a form that the UI never edits.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memo {
    /// The element id; `None` for a new element until its first save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Per field: the value read and the stored bytes of its locations.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, FieldMemo>,
}

impl Memo {
    /// True when nothing is remembered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.fields.is_empty()
    }
}

/// One field as it was read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldMemo {
    /// The value the field was read as.
    pub value: Value,
    /// The stored bytes of its locations (codec specific), written back while the value is unchanged.
    pub raw: Value,
    /// The positions its setters and tags had.
    #[serde(default, skip_serializing_if = "Slots::is_empty")]
    pub slots: Slots,
}

/// Positions inside the ordered lists of an element (setters, support tags), so that the parts different fields
/// write come back in their stored order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slots {
    /// Indices into `setters`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setters: Vec<usize>,
    /// Indices into `supports`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supports: Vec<usize>,
}

impl Slots {
    /// True when no position is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.setters.is_empty() && self.supports.is_empty()
    }
}

/// A plain-language problem with a form, at a form path (`fields.name`, `blocks.2.fields.number`,
/// `children.0.fields.name`; spec 8.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormProblem {
    /// Form path of the field.
    pub path: String,
    /// One plain sentence.
    pub message: String,
}

impl FormProblem {
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// The name and type of a loaded element, for reference fields (the id travels, the name is shown, spec 5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefInfo {
    /// Display name.
    pub name: String,
    /// Element type.
    pub ty: String,
}

/// Finds loaded elements by id.
pub trait Lookup {
    /// The element with this id, if loaded.
    fn find(&self, id: &str) -> Option<RefInfo>;
}

impl Lookup for ContentStore {
    fn find(&self, id: &str) -> Option<RefInfo> {
        self.element(id).map(|e| RefInfo {
            name: e.name.clone(),
            ty: e.ty.clone(),
        })
    }
}

impl Lookup for HashMap<String, RefInfo> {
    fn find(&self, id: &str) -> Option<RefInfo> {
        self.get(id).cloned()
    }
}

/// A lookup that knows nothing (every reference shows as missing).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLookup;

impl Lookup for NoLookup {
    fn find(&self, _id: &str) -> Option<RefInfo> {
        None
    }
}

/// What reading and writing a form needs besides the element.
pub struct FormContext<'a> {
    /// The loaded class names, for conditions (`[warlock:2]` is a class level only when a Warlock is loaded).
    pub conditions: ConditionContext,
    /// Names and types of loaded elements, for reference fields.
    pub lookup: &'a dyn Lookup,
    /// The block representation of rules.
    pub blocks: &'a dyn BlockCodec,
}

impl<'a> FormContext<'a> {
    /// A context over a loaded store: its class names and elements, with the given block codec.
    #[must_use]
    pub fn from_store(store: &'a ContentStore, blocks: &'a dyn BlockCodec) -> Self {
        Self {
            conditions: ConditionContext::new(store.by_type("Class").map(|s| &s.element.name)),
            lookup: store,
            blocks,
        }
    }
}

#[cfg(test)]
mod tests;
