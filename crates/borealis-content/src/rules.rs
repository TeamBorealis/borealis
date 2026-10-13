//! Rules: `grant`, `select`, `stat` (plus `unknown` and forward-compatible custom kinds).

use crate::expr::{EquippedExpr, ExprError, RequirementsExpr, SupportsExpr};
use crate::extra::Extra;
use crate::setter::Setter;
use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Grants an element, by id, to the character while the rule is active.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantRule {
    /// The element id to grant (XML `id`, or the legacy alias `name`).
    pub id: String,
    /// Target element type (informational).
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    /// Active when the progression level is at least this (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Requirements expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,
    /// Spellcasting class name for a granted Spell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spellcasting: Option<String>,
    /// Granted spell is always prepared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared: Option<bool>,
    /// Child `<set>` nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setters: Vec<Setter>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// One `<item id="N">text</item>` of a `type="List"` select.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListItem {
    /// Integer item id.
    pub id: i64,
    /// Item text.
    pub text: String,
    /// Unknown attributes.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// Asks the user to pick one or more elements of a type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectRule {
    /// Candidate element type, or the special value `List`.
    #[serde(rename = "type")]
    pub ty: String,
    /// Label and persistence key.
    pub name: String,
    /// Number of picks (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    /// Active level (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Requirements expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,
    /// Supports expression filtering candidates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports: Option<String>,
    /// The slot may stay empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    /// Preselected element id.
    #[serde(default, rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_id: Option<String>,
    /// Owning spellcasting class name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spellcasting: Option<String>,
    /// Picked spells are always prepared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared: Option<bool>,
    /// XML `allowReplace`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_replace: Option<bool>,
    /// XML `default-behaviour` (kept for round-trip; Borealis does not use it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_behaviour: Option<String>,
    /// XML `existing` (kept for round-trip; Borealis does not use it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing: Option<bool>,
    /// XML `stat` (kept for round-trip; Borealis does not use it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat: Option<String>,
    /// List items (only for `type="List"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ListItem>,
    /// Child `<set>` nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setters: Vec<Setter>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// Contributes to a named statistic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatRule {
    /// Group name, lower-cased and legacy-mapped.
    pub name: String,
    /// Integer text or a group name (optionally negated); raw text when `inline`.
    pub value: String,
    /// Stacking class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonus: Option<String>,
    /// Active level (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Requirements expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,
    /// Equipped expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipped: Option<String>,
    /// XML `not-equipped` (parsed, unused).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_equipped: Option<String>,
    /// The rule defines a text value instead of a number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline: Option<bool>,
    /// Cap: integer text or group name (XML `maximum` or legacy `cap`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<String>,
    /// XML `minimum` (parsed, unused).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<String>,
    /// Label used in the stat breakdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    /// XML `merge` (debugging only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<bool>,
    /// Child `<set>` nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setters: Vec<Setter>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

/// A rule tag the converter did not understand, kept for authoring tools.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnknownRule {
    /// The original tag name.
    pub tag: String,
    /// Attributes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
    /// Inner text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// A rule of a kind this version does not know (for example the 2024 kinds proposed for
/// ruleset profiles). The whole object is preserved.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomRule {
    /// The `kind` string.
    pub kind: String,
    /// Every other field.
    pub fields: Map<String, Value>,
}

/// One rule.
#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    /// `kind: "grant"`
    Grant(GrantRule),
    /// `kind: "select"`
    Select(SelectRule),
    /// `kind: "stat"`
    Stat(StatRule),
    /// `kind: "unknown"`
    Unknown(UnknownRule),
    /// Any other `kind`.
    Custom(CustomRule),
}

/// The three rule kinds of the legacy XML format plus the escape hatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuleKind {
    /// grant
    Grant,
    /// select
    Select,
    /// stat
    Stat,
    /// unknown
    Unknown,
    /// custom (forward-compatible)
    Custom,
}

impl RuleKind {
    /// The name used in reports.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Select => "select",
            Self::Stat => "stat",
            Self::Unknown => "unknown",
            Self::Custom => "custom",
        }
    }
}

impl Rule {
    /// Which kind of rule this is.
    #[must_use]
    pub fn kind(&self) -> RuleKind {
        match self {
            Self::Grant(_) => RuleKind::Grant,
            Self::Select(_) => RuleKind::Select,
            Self::Stat(_) => RuleKind::Stat,
            Self::Unknown(_) => RuleKind::Unknown,
            Self::Custom(_) => RuleKind::Custom,
        }
    }

    /// The raw `requirements` string, when the kind has one.
    #[must_use]
    pub fn requirements(&self) -> Option<&str> {
        match self {
            Self::Grant(r) => r.requirements.as_deref(),
            Self::Select(r) => r.requirements.as_deref(),
            Self::Stat(r) => r.requirements.as_deref(),
            Self::Unknown(_) | Self::Custom(_) => None,
        }
    }

    /// Parse the `requirements` string.
    ///
    /// # Errors
    /// The parser error for malformed text.
    pub fn parse_requirements(&self) -> Result<Option<RequirementsExpr>, ExprError> {
        self.requirements()
            .map_or(Ok(None), RequirementsExpr::parse)
    }

    /// The rule level with the default of 1 applied.
    #[must_use]
    pub fn level(&self) -> u32 {
        match self {
            Self::Grant(r) => r.level,
            Self::Select(r) => r.level,
            Self::Stat(r) => r.level,
            Self::Unknown(_) | Self::Custom(_) => None,
        }
        .unwrap_or(1)
    }
}

impl SelectRule {
    /// Parse the `supports` expression.
    ///
    /// # Errors
    /// The parser error for malformed text.
    pub fn parse_supports(&self) -> Result<Option<SupportsExpr>, ExprError> {
        self.supports
            .as_deref()
            .map_or(Ok(None), SupportsExpr::parse)
    }

    /// True for `type="List"` text list selects.
    #[must_use]
    pub fn is_list(&self) -> bool {
        self.ty == "List"
    }
}

impl StatRule {
    /// Parse the `equipped` expression.
    ///
    /// # Errors
    /// The parser error for malformed text.
    pub fn parse_equipped(&self) -> Result<Option<EquippedExpr>, ExprError> {
        self.equipped
            .as_deref()
            .map_or(Ok(None), EquippedExpr::parse)
    }
}

#[derive(Serialize)]
struct Tagged<'a, T: Serialize> {
    kind: &'static str,
    #[serde(flatten)]
    rule: &'a T,
}

impl Serialize for Rule {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Grant(r) => Tagged {
                kind: "grant",
                rule: r,
            }
            .serialize(serializer),
            Self::Select(r) => Tagged {
                kind: "select",
                rule: r,
            }
            .serialize(serializer),
            Self::Stat(r) => Tagged {
                kind: "stat",
                rule: r,
            }
            .serialize(serializer),
            Self::Unknown(r) => Tagged {
                kind: "unknown",
                rule: r,
            }
            .serialize(serializer),
            Self::Custom(c) => {
                let mut map = serializer.serialize_map(Some(c.fields.len() + 1))?;
                map.serialize_entry("kind", &c.kind)?;
                for (k, v) in &c.fields {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::<String, Value>::deserialize(deserializer)?;
        let kind = match map.remove("kind") {
            Some(Value::String(k)) => k,
            Some(_) => return Err(D::Error::custom("rule `kind` must be a string")),
            None => return Err(D::Error::missing_field("kind")),
        };
        let body = Value::Object(map);
        let ctx = |e: serde_json::Error| D::Error::custom(format!("invalid {kind} rule: {e}"));
        Ok(match kind.as_str() {
            "grant" => Self::Grant(serde_json::from_value(body).map_err(ctx)?),
            "select" => Self::Select(serde_json::from_value(body).map_err(ctx)?),
            "stat" => Self::Stat(serde_json::from_value(body).map_err(ctx)?),
            "unknown" => Self::Unknown(serde_json::from_value(body).map_err(ctx)?),
            _ => {
                let Value::Object(fields) = body else {
                    unreachable!("body was built from a map")
                };
                Self::Custom(CustomRule { kind, fields })
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kind_is_serialized_first_and_round_trips() {
        let rule = Rule::Grant(GrantRule {
            id: "ID_X".into(),
            ty: Some("Proficiency".into()),
            ..GrantRule::default()
        });
        let text = serde_json::to_string(&rule).unwrap();
        assert_eq!(text, r#"{"kind":"grant","id":"ID_X","type":"Proficiency"}"#);
        assert_eq!(serde_json::from_str::<Rule>(&text).unwrap(), rule);
    }

    #[test]
    fn all_kinds_round_trip() {
        let rules = json!([
            {"kind":"select","type":"List","name":"Ideal","items":[{"id":1,"text":"a"}]},
            {"kind":"stat","name":"ac:misc","value":"1","bonus":"fighting style","equipped":"[armor:any]","inline":true},
            {"kind":"unknown","tag":"inline","attributes":{"a":"b"},"text":"t"},
            {"kind":"select.ability_distribution","pool":["str","dex"],"cap":20}
        ]);
        let parsed: Vec<Rule> = serde_json::from_value(rules.clone()).unwrap();
        assert_eq!(parsed[0].kind(), RuleKind::Select);
        assert_eq!(parsed[1].kind(), RuleKind::Stat);
        assert_eq!(parsed[2].kind(), RuleKind::Unknown);
        assert_eq!(parsed[3].kind(), RuleKind::Custom);
        assert_eq!(serde_json::to_value(&parsed).unwrap(), rules);
        assert_eq!(RuleKind::Custom.as_str(), "custom");
    }

    #[test]
    fn rejects_bad_rules() {
        assert!(serde_json::from_str::<Rule>(r#"{"id":"x"}"#).is_err());
        assert!(serde_json::from_str::<Rule>(r#"{"kind":3}"#).is_err());
        assert!(serde_json::from_str::<Rule>(r#"{"kind":"grant","id":"x","bogus":1}"#).is_err());
        assert!(serde_json::from_str::<Rule>(r#"{"kind":"stat","name":"x"}"#).is_err());
    }

    #[test]
    fn helpers_parse_expressions() {
        let rule = Rule::Stat(StatRule {
            name: "ac".into(),
            value: "1".into(),
            requirements: Some("![str:13]".into()),
            equipped: Some("[armor:any]".into()),
            level: Some(3),
            ..StatRule::default()
        });
        assert_eq!(rule.level(), 3);
        assert!(rule.parse_requirements().unwrap().is_some());
        let Rule::Stat(stat) = &rule else {
            unreachable!()
        };
        assert!(stat.parse_equipped().unwrap().is_some());

        let select = SelectRule {
            ty: "List".into(),
            name: "n".into(),
            supports: Some("A, B".into()),
            ..SelectRule::default()
        };
        assert!(select.is_list());
        assert_eq!(
            select.parse_supports().unwrap().unwrap().tags(),
            vec!["A", "B"]
        );
        assert_eq!(Rule::Select(select).level(), 1);
    }
}
