//! Class spell lists from `generated/gendata-spell-source-lookup.json`.

use serde_json::Value;
use std::collections::BTreeSet;

/// Which classes (and subclasses) have a spell on their list.
#[derive(Debug, Default, Clone)]
pub struct SpellClassLookup {
    root: Option<Value>,
}

fn collect_keys(node: Option<&Value>, depth: usize, out: &mut BTreeSet<String>) {
    // class:    { classSource: { ClassName: ... } }               -> keys at depth 1
    // subclass: { classSource: { ClassName: { subSource: { SubShort: {name} } } } } -> name at depth 4
    let Some(Value::Object(map)) = node else {
        return;
    };
    for (_, level1) in map {
        let Value::Object(level1) = level1 else {
            continue;
        };
        if depth == 1 {
            out.extend(level1.keys().cloned());
            continue;
        }
        for (_, level2) in level1 {
            let Value::Object(level2) = level2 else {
                continue;
            };
            for (_, level3) in level2 {
                let Value::Object(level3) = level3 else {
                    continue;
                };
                for (short, info) in level3 {
                    let name = info.get("name").and_then(Value::as_str).unwrap_or(short);
                    out.insert(name.to_string());
                }
            }
        }
    }
}

impl SpellClassLookup {
    /// Wrap the parsed lookup file (or nothing when the file is missing).
    #[must_use]
    pub fn new(root: Option<Value>) -> Self {
        Self { root }
    }

    /// True if a lookup file was supplied.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.root.is_some()
    }

    fn entry(&self, name: &str, source: &str) -> Option<&Value> {
        self.root
            .as_ref()?
            .get(source.to_lowercase())?
            .get(name.to_lowercase())
    }

    /// Class names that have the spell on their list, sorted.
    #[must_use]
    pub fn classes(&self, name: &str, source: &str) -> Vec<String> {
        let mut out = BTreeSet::new();
        collect_keys(
            self.entry(name, source).and_then(|e| e.get("class")),
            1,
            &mut out,
        );
        out.into_iter().collect()
    }

    /// Display names of the subclasses that add the spell to a list, sorted.
    #[must_use]
    pub fn subclasses(&self, name: &str, source: &str) -> Vec<String> {
        let mut out = BTreeSet::new();
        collect_keys(
            self.entry(name, source).and_then(|e| e.get("subclass")),
            4,
            &mut out,
        );
        out.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_class_and_subclass_lists() {
        let lookup = SpellClassLookup::new(Some(json!({
            "phb": {"fireball": {
                "class": {"PHB": {"Sorcerer": true, "Wizard": true}, "TCE": {"Artificer": true}},
                "subclass": {"XPHB": {"Fighter": {"XPHB": {"Eldritch Knight": {"name": "Eldritch Knight"}}}}}
            }}
        })));
        assert!(lookup.is_available());
        assert_eq!(
            lookup.classes("Fireball", "PHB"),
            vec!["Artificer", "Sorcerer", "Wizard"]
        );
        assert_eq!(
            lookup.subclasses("Fireball", "phb"),
            vec!["Eldritch Knight"]
        );
        assert!(lookup.classes("Nope", "PHB").is_empty());
        assert!(lookup.classes("Fireball", "XGE").is_empty());
        let none = SpellClassLookup::new(None);
        assert!(!none.is_available());
        assert!(none.classes("x", "y").is_empty());
    }
}
