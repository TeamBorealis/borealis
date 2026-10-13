//! The lossless escape hatch: anything the XML had that the schema has no field for.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An unknown XML child node, kept as `{tag, attributes, text}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnknownNode {
    /// The original tag name.
    pub tag: String,
    /// The node's attributes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
    /// Trimmed inner text (or inner XML when the node had child elements).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Unknown attributes (original names, string values) and unknown child nodes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extra {
    /// Unknown XML attributes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
    /// Unknown child nodes in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<UnknownNode>,
}

impl Extra {
    /// True when nothing is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty() && self.children.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_extra_is_skipped_by_parents_and_round_trips() {
        let mut extra = Extra::default();
        assert!(extra.is_empty());
        extra.attributes.insert("x".into(), "1".into());
        extra.children.push(UnknownNode {
            tag: "foo".into(),
            attributes: BTreeMap::new(),
            text: Some("bar".into()),
        });
        let json = serde_json::to_string(&extra).unwrap();
        assert_eq!(serde_json::from_str::<Extra>(&json).unwrap(), extra);
        assert!(!extra.is_empty());
    }
}
