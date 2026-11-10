//! The shapes field values have in [`super::FormState::fields`] (and on the wire, `HomebrewForm.fields`), by control:
//!
//! | Control | Value |
//! |---|---|
//! | `text`, `multiline`, `combo` | string (`""` = not set) |
//! | `check`, `toggle` | boolean |
//! | `tags`, `chips` | list of strings |
//! | `reference` | [`RefValue`] or `null` |
//! | `condition` | `ExprValue<Condition>` (WB-02, `expr_build`) |
//! | `richText` | `borealis_content::rich::Doc` (WB-03; ProseMirror JSON) |
//! | `sheet` | [`SheetValue`] or `null` |
//! | `setters` (expert rows) | list of [`SetterRow`] |
//! | `mapsTo` | [`MapsToValue`] or `null` |
//!
//! TS mirrors: `ui/src/api/types/homebrew.ts` (`HomebrewRef`, `HomebrewSheetValue`, `HomebrewSetterRow`,
//! `HomebrewMapsToValue`), checked by `tests/form_dto.rs` and its UI test.

use super::{Lookup, RefInfo};
use borealis_content::extra::Extra;
use borealis_content::{MapKind, MapsTo, Sheet, SheetEntry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A reference held by a field: the id travels, the name is shown (`HomebrewRef`, spec 5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefValue {
    /// Element id (never shown).
    pub id: String,
    /// Name to show; empty when the element is not loaded.
    pub name: String,
    /// Element type; empty when the element is not loaded.
    #[serde(rename = "type")]
    pub ty: String,
    /// The element is not loaded ("Missing: <name>").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

impl RefValue {
    /// The reference to `id`, named from `lookup`.
    #[must_use]
    pub fn resolve(id: &str, lookup: &dyn Lookup) -> Self {
        match lookup.find(id) {
            Some(RefInfo { name, ty }) => Self {
                id: id.to_string(),
                name,
                ty,
                missing: false,
            },
            None => Self {
                id: id.to_string(),
                name: String::new(),
                ty: String::new(),
                missing: true,
            },
        }
    }
}

/// One setter row of the expert "Other properties" list (spec 4.12): name, value and attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetterRow {
    /// Setter name.
    pub name: String,
    /// Value text.
    pub value: String,
    /// Attributes (`currency`, `lb`, `type`...).
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

/// One sheet line (`sheet.descriptions[]`): text with `{{stat}}` placeholders, from a level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetLine {
    /// The text.
    pub text: String,
    /// From this level (absent = 1).
    #[serde(default)]
    pub level: Option<u32>,
    /// Usage override ("1/Long Rest").
    #[serde(default)]
    pub usage: Option<String>,
    /// Action override.
    #[serde(default)]
    pub action: Option<String>,
    /// Unknown attributes of the stored line: sent back unchanged.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub kept: Extra,
}

/// "On the character sheet" (spec 3.2): `sheet.display`, `alt`, `usage`, `action`, `descriptions[]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetValue {
    /// Listed on the sheet (absent = yes).
    #[serde(default)]
    pub display: Option<bool>,
    /// Short name.
    #[serde(default)]
    pub alt: Option<String>,
    /// Usage ("1/Long Rest").
    #[serde(default)]
    pub usage: Option<String>,
    /// Action (Action, Bonus Action, Reaction...).
    #[serde(default)]
    pub action: Option<String>,
    /// Sheet lines in order.
    #[serde(default)]
    pub lines: Vec<SheetLine>,
}

impl SheetValue {
    /// The editable parts of a stored sheet (its own `extra` is a preserved part).
    #[must_use]
    pub fn from_sheet(sheet: &Sheet) -> Self {
        Self {
            display: sheet.display,
            alt: sheet.alt.clone(),
            usage: sheet.usage.clone(),
            action: sheet.action.clone(),
            lines: sheet
                .descriptions
                .iter()
                .map(|d| SheetLine {
                    text: d.text.clone(),
                    level: d.level,
                    usage: d.usage.clone(),
                    action: d.action.clone(),
                    kept: d.extra.clone(),
                })
                .collect(),
        }
    }

    /// The sheet this value writes, with the sheet's kept `extra`.
    #[must_use]
    pub fn to_sheet(&self, extra: Extra) -> Sheet {
        let text = |t: &Option<String>| t.clone().filter(|s| !s.trim().is_empty());
        Sheet {
            display: self.display,
            alt: text(&self.alt),
            usage: text(&self.usage),
            action: text(&self.action),
            descriptions: self
                .lines
                .iter()
                .filter(|l| !l.text.trim().is_empty())
                .map(|l| SheetEntry {
                    text: l.text.trim().to_string(),
                    level: l.level,
                    usage: text(&l.usage),
                    action: text(&l.action),
                    extra: l.kept.clone(),
                })
                .collect(),
            extra,
        }
    }

    /// True when the value writes no sheet at all.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.to_sheet(Extra::default()) == Sheet::default()
    }
}

/// "2014/2024 counterpart" (expert, spec 3.2): `maps_to`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapsToValue {
    /// The 2014 counterpart.
    #[serde(default)]
    pub dnd2014: Option<RefValue>,
    /// The 2024 counterpart.
    #[serde(default)]
    pub dnd2024: Option<RefValue>,
    /// `equivalent`, `approximate` or `none`.
    #[serde(default)]
    pub kind: Option<MapKind>,
}

impl MapsToValue {
    /// The value of a stored mapping.
    #[must_use]
    pub fn from_maps_to(m: &MapsTo, lookup: &dyn Lookup) -> Self {
        Self {
            dnd2014: m.dnd2014.as_deref().map(|id| RefValue::resolve(id, lookup)),
            dnd2024: m.dnd2024.as_deref().map(|id| RefValue::resolve(id, lookup)),
            kind: m.kind,
        }
    }

    /// The mapping this value writes, `None` when empty.
    #[must_use]
    pub fn to_maps_to(&self) -> Option<MapsTo> {
        let id = |r: &Option<RefValue>| r.as_ref().map(|r| r.id.clone()).filter(|i| !i.is_empty());
        let m = MapsTo {
            dnd2014: id(&self.dnd2014),
            dnd2024: id(&self.dnd2024),
            kind: self.kind,
        };
        (m != MapsTo::default()).then_some(m)
    }
}
