//! `<sheet>` and `<compendium>`.

use crate::expr::Template;
use crate::extra::Extra;
use serde::{Deserialize, Serialize};

/// One sheet text entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetEntry {
    /// Trimmed text; may contain `{{stat}}` or `$(stat)` placeholders.
    pub text: String,
    /// Progression level this entry applies from (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Usage override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    /// Action type override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Unknown attributes.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl SheetEntry {
    /// The effective level (default 1).
    #[must_use]
    pub fn effective_level(&self) -> u32 {
        self.level.unwrap_or(1)
    }

    /// The text split into literal runs and placeholders.
    #[must_use]
    pub fn template(&self) -> Template {
        Template::parse(&self.text)
    }
}

/// How the element appears on the printed sheet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// Include in the sheet features list (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    /// Alternate short name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    /// Default usage text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    /// Action type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Text entries in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<SheetEntry>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

impl Sheet {
    /// `display` with its default applied.
    #[must_use]
    pub fn is_displayed(&self) -> bool {
        self.display.unwrap_or(true)
    }
}

/// `<compendium display="bool"/>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compendium {
    /// Show in the compendium browser (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    /// Unknown attributes and children.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_template() {
        let sheet = Sheet::default();
        assert!(sheet.is_displayed());
        let entry = SheetEntry {
            text: "DC {{ki:dc}}".into(),
            ..SheetEntry::default()
        };
        assert_eq!(entry.effective_level(), 1);
        assert_eq!(entry.template().placeholders().len(), 1);
        let json = serde_json::to_string(&Sheet {
            display: Some(false),
            ..Sheet::default()
        })
        .unwrap();
        assert_eq!(json, r#"{"display":false}"#);
    }
}
