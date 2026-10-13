//! `<spellcasting>` nodes.

use crate::expr::SupportsExpr;
use crate::extra::Extra;
use serde::{Deserialize, Serialize};

/// A `<list>` or `<extend>` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellcastingList {
    /// Supports expression over Spell elements, or a single spell id for `<extend>`.
    pub supports: String,
    /// `known="bool"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known: Option<bool>,
    /// Unknown attributes.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl SpellcastingList {
    /// A list with just the expression.
    #[must_use]
    pub fn new(supports: impl Into<String>) -> Self {
        Self {
            supports: supports.into(),
            ..Self::default()
        }
    }

    /// Parse the supports expression.
    ///
    /// # Errors
    /// Returns the parser error for malformed text.
    pub fn expr(&self) -> Result<Option<SupportsExpr>, crate::expr::ExprError> {
        SupportsExpr::parse(&self.supports)
    }

    /// True when the text is a single spell id rather than an expression.
    #[must_use]
    pub fn is_spell_id(&self) -> bool {
        let t = self.supports.trim();
        t.starts_with("ID_") && !t.contains(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
    }
}

/// A spellcasting declaration or extension.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spellcasting {
    /// Spellcasting class name (absent = `N/A`, allowed only with `all`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Spellcasting ability name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ability: Option<String>,
    /// The class prepares spells from its list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepare: Option<bool>,
    /// One known spell may be swapped per level-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_replace: Option<bool>,
    /// This node extends an existing class's list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extend: Option<bool>,
    /// The extension applies to every spellcasting class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// The spell list expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<SpellcastingList>,
    /// Extension entries in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<SpellcastingList>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_helpers() {
        assert!(SpellcastingList::new("ID_PHB_SPELL_COMMAND").is_spell_id());
        assert!(!SpellcastingList::new("Wizard").is_spell_id());
        assert!(!SpellcastingList::new("ID_A||ID_B").is_spell_id());
        let e = SpellcastingList::new("Wizard").expr().unwrap().unwrap();
        assert_eq!(e.tags(), vec!["Wizard"]);
    }
}
