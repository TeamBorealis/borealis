//! `<set name="..">value</set>` typed name/value properties.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One setter. Lookup by name is case-insensitive and returns the first match; duplicates
/// are allowed and preserved in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setter {
    /// The setter name; case preserved.
    pub name: String,
    /// Trimmed inner text.
    pub value: String,
    /// Every XML attribute other than `name` (`currency`, `lb`, `type`, `modifier`, ...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

impl Setter {
    /// A setter without attributes.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            attributes: BTreeMap::new(),
        }
    }

    /// Case-insensitive attribute lookup.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Interpret the value as a boolean the way .NET `Convert.ToBoolean` does.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        parse_bool(&self.value)
    }
}

/// `true`/`false` in any letter case, surrounding whitespace tolerated.
#[must_use]
pub fn parse_bool(text: &str) -> Option<bool> {
    let t = text.trim();
    if t.eq_ignore_ascii_case("true") {
        Some(true)
    } else if t.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// First setter with this name, ignoring ASCII case.
#[must_use]
pub fn find<'a>(setters: &'a [Setter], name: &str) -> Option<&'a Setter> {
    setters.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

/// All setters with this name, ignoring ASCII case, in order.
pub fn find_all<'a>(setters: &'a [Setter], name: &'a str) -> impl Iterator<Item = &'a Setter> {
    setters
        .iter()
        .filter(move |s| s.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive_first_match() {
        let setters = vec![
            Setter::new("Names", "a"),
            Setter::new("names", "b"),
            Setter::new("hd", "d10"),
        ];
        assert_eq!(find(&setters, "NAMES").unwrap().value, "a");
        assert_eq!(find_all(&setters, "names").count(), 2);
        assert!(find(&setters, "missing").is_none());
    }

    #[test]
    fn booleans() {
        assert_eq!(parse_bool(" TRUE "), Some(true));
        assert_eq!(parse_bool("False"), Some(false));
        assert_eq!(parse_bool("yes"), None);
        assert_eq!(parse_bool("1"), None);
        let mut s = Setter::new("x", "true");
        s.attributes.insert("Type".into(), "male".into());
        assert_eq!(s.as_bool(), Some(true));
        assert_eq!(s.attribute("type"), Some("male"));
    }
}
