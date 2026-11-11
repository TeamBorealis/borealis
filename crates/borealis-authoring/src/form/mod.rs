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
mod child_refs;
mod codec;
pub mod descriptor;
mod item_codec;
mod item_kinds;
pub mod item_value;
pub mod kinds;
mod other;
mod paths;
pub mod preserved;
mod read;
mod repeat;
mod subclass;
mod ui;
pub mod value;
mod write;

pub use append::{append_to_form, form_to_append, APPEND_KIND};
pub use block::{Block, BlockCodec, BlockMemo};
pub use block_codec::CatalogBlockCodec;
pub(crate) use block_codec::INCOMPLETE;
pub use child_refs::{add_child, new_child_key, ChildEntry, ParentInfo};
pub use codec::{
    CasterField, ClassField, FeatureField, LevelCounts, ProficiencyGroup, SkillChoice, SmallField,
    SubclassField, TableColumn,
};
pub use descriptor::descriptors;
pub use kinds::new_class_form;
pub use kinds::{
    append_kind, kind, kind_for_element, kind_for_type, kinds, Control, FieldDef, KindDef,
    KindGroup, CLASS_FEATURE_KIND, CLASS_KIND, CLASS_OPTION_KIND, OTHER_KIND,
};
pub use other::{
    open_types, setter_names, type_refusal, TypeRefusal, ELEMENT_TYPE_PICKER, OPEN_TYPES,
    SETTER_NAME_PICKER,
};
pub use paths::{check_fields, form_path_for_issue, form_path_for_location};
pub use preserved::{KeptPart, PreservedPart};
pub use read::{element_group_to_form, element_to_form};
pub use repeat::AGAIN_AT;
pub use subclass::{refresh_subclass, SUBCLASS_FEATURE_KIND, SUBCLASS_KIND};
pub use ui::{assign_ids, merge_ui, refresh_blocks, to_ui};
pub use write::{form_to_element, form_to_elements};

use crate::expr_build::condition::ConditionContext;
use borealis_content::{ContentStore, Element};
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
    /// The places of the element's stored rules, when fields took some of them (a species' size and speed, an
    /// armor's AC): one entry per stored rule, the key of the field that took it (its [`Slots::rules`]) or `""` for a
    /// block's. Empty when every rule is a block. Writing reads it only for a field whose memo is gone, so the stored
    /// order survives a canonical write.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout: Vec<String>,
    /// A class's multiclass section id (spec 5.3: `ID_<CODE>_MULTICLASS_<CLASS>`), read from the element or given
    /// with the class id; `None` for a new class (and a copy) until its first save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiclass_id: Option<String>,
}

impl Memo {
    /// True when nothing is remembered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.id.is_none()
            && self.fields.is_empty()
            && self.layout.is_empty()
            && self.multiclass_id.is_none()
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
    /// Indices into `rules`, for rules a field edits (an armor's base AC); the other rules are blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<usize>,
}

impl Slots {
    /// True when no position is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.setters.is_empty() && self.supports.is_empty() && self.rules.is_empty()
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

    /// The whole loaded element with this id, when the lookup holds elements (a subclass reads its class's subclass
    /// choice, spec 4.7). Lookups of names only answer `None`.
    fn loaded(&self, _id: &str) -> Option<&Element> {
        None
    }

    /// Every loaded element of type `ty`, when the lookup holds elements (empty otherwise).
    fn loaded_of_type(&self, _ty: &str) -> Vec<&Element> {
        Vec::new()
    }

    /// The element types loaded elements have (each once, any order), when the lookup holds elements (empty
    /// otherwise): the generic form's Type combo offers the open ones (spec 4.12).
    fn loaded_types(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Lookup for ContentStore {
    fn find(&self, id: &str) -> Option<RefInfo> {
        self.element(id).map(|e| RefInfo {
            name: e.name.clone(),
            ty: e.ty.clone(),
        })
    }

    fn loaded(&self, id: &str) -> Option<&Element> {
        self.element(id)
    }

    fn loaded_of_type(&self, ty: &str) -> Vec<&Element> {
        self.by_type(ty).map(|s| &s.element).collect()
    }

    fn loaded_types(&self) -> Vec<String> {
        let types: std::collections::BTreeSet<&str> =
            self.iter().map(|s| s.element.ty.as_str()).collect();
        types.into_iter().map(str::to_string).collect()
    }
}

/// A lookup over another one plus some elements that are not loaded (a pack's unsaved drafts): those are found first.
pub struct WithElements<'a> {
    /// The loaded content.
    pub base: &'a dyn Lookup,
    /// The extra elements.
    pub extra: &'a [Element],
}

impl Lookup for WithElements<'_> {
    fn find(&self, id: &str) -> Option<RefInfo> {
        self.extra
            .iter()
            .find(|e| e.id == id)
            .map(|e| RefInfo {
                name: e.name.clone(),
                ty: e.ty.clone(),
            })
            .or_else(|| self.base.find(id))
    }

    fn loaded(&self, id: &str) -> Option<&Element> {
        self.extra
            .iter()
            .find(|e| e.id == id)
            .or_else(|| self.base.loaded(id))
    }

    fn loaded_of_type(&self, ty: &str) -> Vec<&Element> {
        let mut out: Vec<&Element> = self.extra.iter().filter(|e| e.ty == ty).collect();
        out.extend(
            self.base
                .loaded_of_type(ty)
                .into_iter()
                .filter(|e| !self.extra.iter().any(|x| x.id == e.id)),
        );
        out
    }

    fn loaded_types(&self) -> Vec<String> {
        let mut types = self.base.loaded_types();
        for e in self.extra {
            if !types.contains(&e.ty) {
                types.push(e.ty.clone());
            }
        }
        types
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
