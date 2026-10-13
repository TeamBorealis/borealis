//! Elements and appends.

use crate::expr::{ExprError, RequirementsExpr};
use crate::extra::Extra;
use crate::rules::Rule;
use crate::ruleset::Ruleset;
use crate::setter::{self, Setter};
use crate::sheet::{Compendium, Sheet};
use crate::spellcasting::Spellcasting;
use crate::types::KnownType;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// `<equipment name="..">` with a description (kept for round-trip, never displayed).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equipment {
    /// Equipment note name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Description HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// Element-level `<source>` child (kept for round-trip; Borealis does not use it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    /// Source name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Source element id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// Nested `<multiclass>` of a Class element. The loader synthesises the sibling
/// `Multiclass` element from it; it is not stored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Multiclass {
    /// The id of the synthesized Multiclass element.
    pub id: String,
    /// Display text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prerequisite: String,
    /// Requirements expression.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub requirements: String,
    /// Description HTML prepended to the class description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Setters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setters: Vec<Setter>,
    /// Extra rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// One `<item amount="n">ID</item>` of an equipment pack.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractItem {
    /// Item element id.
    pub id: String,
    /// Quantity (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<u32>,
    /// Unknown or unparsable attributes.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// Item `<extract>`: the contents of an equipment pack.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extract {
    /// Contained items in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ExtractItem>,
    /// Description HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// How a 2014 element corresponds to a 2024 one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MapKind {
    /// Same thing under the other ruleset.
    Equivalent,
    /// Close but not identical.
    Approximate,
    /// No counterpart.
    None,
}

/// Cross-ruleset mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapsTo {
    /// Counterpart id in the 2014 ruleset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dnd2014: Option<String>,
    /// Counterpart id in the 2024 ruleset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dnd2024: Option<String>,
    /// How close the counterpart is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MapKind>,
}

/// One content element.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    /// Unique, case-sensitive id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Open type string.
    #[serde(rename = "type")]
    pub ty: String,
    /// Name (not id) of a Source element, or `Core`/`Internal`.
    pub source: String,
    /// Target ruleset; absent means "the file's ruleset".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruleset: Option<Ruleset>,
    /// Explicit support tags in XML order (derived tags are not stored).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supports: Vec<String>,
    /// Display-only prerequisite text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prerequisite: String,
    /// Requirements expression.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub requirements: String,
    /// HTML fragment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Sheet presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<Sheet>,
    /// Compendium visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compendium: Option<Compendium>,
    /// Typed name/value properties, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setters: Vec<Setter>,
    /// Rules, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    /// Spell list declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spellcasting: Option<Spellcasting>,
    /// Equipment note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment: Option<Equipment>,
    /// Element-level source reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<SourceRef>,
    /// Multiclass definition (Class elements).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiclass: Option<Multiclass>,
    /// Equipment pack contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
    /// Cross-ruleset mapping (2024 support).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maps_to: Option<MapsTo>,
    /// Previous id or name this element replaced (2024 renames).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
    /// Legacy type alias, for example `Race` on a `Species`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_alias: Option<String>,
    /// Per-ruleset field overrides, for example `{"dnd2024": {"damage": "1d8"}}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profile_overrides: BTreeMap<String, BTreeMap<String, Value>>,
    /// Structured type specific data for 2024 era element types (opaque to this crate).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, Value>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl Element {
    /// A bare element with the four required header fields.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        ty: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            ty: ty.into(),
            source: source.into(),
            ..Self::default()
        }
    }

    /// The type as a [`KnownType`], if recognised.
    #[must_use]
    pub fn known_type(&self) -> Option<KnownType> {
        KnownType::parse(&self.ty)
    }

    /// First setter with this name (case-insensitive).
    #[must_use]
    pub fn setter(&self, name: &str) -> Option<&Setter> {
        setter::find(&self.setters, name)
    }

    /// Value of the first setter with this name.
    #[must_use]
    pub fn setter_value(&self, name: &str) -> Option<&str> {
        self.setter(name).map(|s| s.value.as_str())
    }

    /// True if the explicit support tags contain `tag` (case-sensitive).
    #[must_use]
    pub fn has_support(&self, tag: &str) -> bool {
        self.supports.iter().any(|s| s == tag)
    }

    /// The element's own rules followed by those of its `multiclass` node.
    pub fn all_rules(&self) -> impl Iterator<Item = &Rule> {
        self.rules
            .iter()
            .chain(self.multiclass.iter().flat_map(|m| m.rules.iter()))
    }

    /// Parse the element `requirements`.
    ///
    /// # Errors
    /// The parser error for malformed text.
    pub fn parse_requirements(&self) -> Result<Option<RequirementsExpr>, ExprError> {
        RequirementsExpr::parse(&self.requirements)
    }
}

/// `<append id=".." [type=".."]>`: extends an existing element after loading.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Append {
    /// The target element id.
    pub id: String,
    /// Optional parser selector (ignored by the merge).
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    /// Tags added to the target's supports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supports: Vec<String>,
    /// Setters added when the target has none with that name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setters: Vec<Setter>,
    /// Rules appended after the target's rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    /// Adopted only if the target has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spellcasting: Option<Spellcasting>,
    /// Unknown attributes and children (including parts that have no effect in appends).
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{GrantRule, StatRule};

    #[test]
    fn minimal_element_serializes_compactly() {
        let e = Element::new("ID_A", "A", "Spell", "Core");
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(
            json,
            r#"{"id":"ID_A","name":"A","type":"Spell","source":"Core"}"#
        );
        assert_eq!(serde_json::from_str::<Element>(&json).unwrap(), e);
        assert_eq!(e.known_type(), Some(KnownType::Spell));
    }

    #[test]
    fn helpers() {
        let mut e = Element::new("ID_A", "A", "Class", "Core");
        e.setters.push(Setter::new("HD", "d10"));
        e.supports = vec!["X".into()];
        e.rules.push(Rule::Stat(StatRule::default()));
        e.multiclass = Some(Multiclass {
            id: "ID_M".into(),
            rules: vec![Rule::Grant(GrantRule::default())],
            ..Multiclass::default()
        });
        assert_eq!(e.setter_value("hd"), Some("d10"));
        assert!(e.has_support("X"));
        assert!(!e.has_support("x"));
        assert_eq!(e.all_rules().count(), 2);
        assert!(e.parse_requirements().unwrap().is_none());
    }

    #[test]
    fn rejects_unknown_fields() {
        let bad = r#"{"id":"A","name":"A","type":"T","source":"S","wat":1}"#;
        assert!(serde_json::from_str::<Element>(bad).is_err());
    }

    #[test]
    fn ruleset_fields_round_trip() {
        let json = r#"{"id":"A","name":"A","type":"Species","source":"S","ruleset":"dnd2024",
            "maps_to":{"dnd2014":"ID_RACE","kind":"approximate"},"renamed_from":"ID_OLD",
            "profile_overrides":{"dnd2024":{"damage":"1d8"}},"data":{"category":"Origin"}}"#;
        let e: Element = serde_json::from_str(json).unwrap();
        assert_eq!(e.ruleset, Some(Ruleset::Dnd2024));
        assert_eq!(e.maps_to.as_ref().unwrap().kind, Some(MapKind::Approximate));
        let back: Element = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }
}
