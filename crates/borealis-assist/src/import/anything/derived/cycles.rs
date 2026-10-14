//! No grant of the import may close a cycle: an element that grants itself, or grants an
//! element that (through the library or the import) grants it back, is malformed content. The
//! engine cuts such chains, but the pack must not carry them. Each draft grant (element rules
//! and `appends`) is checked in order against the library's grants plus the draft grants kept
//! so far; one that would close a cycle is dropped and noted in the report.

use std::collections::{HashMap, HashSet};

use borealis_content::{ContentStore, Rule};
use serde_json::Value;

use super::{str_at, Derived};

/// Grant edges by granting element id.
type Edges = HashMap<String, Vec<String>>;

fn library_edges(store: &ContentStore) -> Edges {
    let mut edges = Edges::new();
    for s in store.iter() {
        for r in &s.element.rules {
            if let Rule::Grant(g) = r {
                edges
                    .entry(s.element.id.clone())
                    .or_default()
                    .push(g.id.clone());
            }
        }
    }
    edges
}

/// Does `from` reach `to` through grants (or is it `to`)?
fn reaches(edges: &Edges, from: &str, to: &str) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        if let Some(next) = edges.get(n) {
            stack.extend(next.iter().map(String::as_str));
        }
    }
    false
}

/// Drop the grants of `owner`'s `rules` that would close a cycle; keep the rest as edges.
fn filter_rules(
    owner: &str,
    rules: &mut Vec<Value>,
    edges: &mut Edges,
    dropped: &mut Vec<(String, String)>,
) {
    rules.retain(|r| {
        if str_at(r, "kind") != "grant" {
            return true;
        }
        let target = str_at(r, "id");
        if target.is_empty() {
            return true;
        }
        if reaches(edges, target, owner) {
            dropped.push((owner.to_string(), target.to_string()));
            return false;
        }
        edges
            .entry(owner.to_string())
            .or_default()
            .push(target.to_string());
        true
    });
}

fn rules_of(v: &mut Value) -> Option<&mut Vec<Value>> {
    v.get_mut("rules")?.as_array_mut()
}

/// Remove every draft grant that closes a grant cycle (see the module docs).
pub(super) fn break_grant_cycles(store: &ContentStore, d: &mut Derived) {
    let mut edges = library_edges(store);
    let mut dropped = Vec::new();
    for doc in d.docs.iter_mut().chain(d.extra.iter_mut()) {
        let owner = str_at(doc, "id").to_string();
        if let Some(rules) = rules_of(doc) {
            filter_rules(&owner, rules, &mut edges, &mut dropped);
        }
    }
    for a in &mut d.appends {
        let owner = str_at(a, "id").to_string();
        if let Some(rules) = rules_of(a) {
            filter_rules(&owner, rules, &mut edges, &mut dropped);
        }
    }
    d.appends.retain(|a| {
        !a.get("rules")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    });
    for (owner, target) in dropped {
        let message = if owner == target {
            "a grant of the element itself was dropped (it would recurse without end)".to_string()
        } else {
            format!("its grant of \"{target}\" was dropped: \"{target}\" already grants it, directly or through other elements (a grant cycle)")
        };
        d.note(&owner, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::ContentStoreBuilder;
    use serde_json::json;

    fn grant(id: &str) -> Value {
        json!({"kind": "grant", "id": id, "type": "Proficiency"})
    }

    #[test]
    fn self_grants_and_two_element_cycles_are_dropped() {
        let store = ContentStoreBuilder::new().build();
        let mut d = Derived {
            docs: vec![
                json!({"id": "ID_T_A", "rules": [grant("ID_T_A"), grant("ID_T_B")]}),
                json!({"id": "ID_T_B", "rules": [grant("ID_T_A"), grant("ID_T_C")]}),
                json!({"id": "ID_T_C"}),
            ],
            appends: vec![
                json!({"id": "ID_T_C", "type": "Proficiency", "rules": [grant("ID_T_C")]}),
                json!({"id": "ID_T_X", "type": "Proficiency", "rules": [grant("ID_T_C")]}),
            ],
            ..Derived::default()
        };
        break_grant_cycles(&store, &mut d);
        let ids = |v: &Value| -> Vec<String> {
            v.get("rules")
                .and_then(Value::as_array)
                .map(|r| r.iter().map(|g| str_at(g, "id").to_string()).collect())
                .unwrap_or_default()
        };
        assert_eq!(ids(&d.docs[0]), ["ID_T_B"]);
        assert_eq!(ids(&d.docs[1]), ["ID_T_C"]);
        assert_eq!(d.appends.len(), 1, "the emptied self-append is gone");
        assert_eq!(str_at(&d.appends[0], "id"), "ID_T_X");
        assert_eq!(d.notes.len(), 3);
    }

    #[test]
    fn a_grant_closing_a_cycle_through_the_library_is_dropped() {
        let file = borealis_content::ContentFile::from_json_str(
            &json!({"format": "borealis-content/1", "elements": [
                {"id": "ID_T_LIB", "name": "Lib", "type": "Proficiency", "source": "T",
                 "rules": [grant("ID_T_NEW")]}]})
            .to_string(),
        )
        .expect("file");
        let store = ContentStoreBuilder::new()
            .add_file("lib.json", borealis_content::PackOrigin::Custom, file)
            .build();
        let mut d = Derived {
            docs: vec![json!({"id": "ID_T_NEW", "rules": [grant("ID_T_LIB")]})],
            ..Derived::default()
        };
        break_grant_cycles(&store, &mut d);
        assert!(d.docs[0]["rules"].as_array().is_some_and(Vec::is_empty));
    }
}
