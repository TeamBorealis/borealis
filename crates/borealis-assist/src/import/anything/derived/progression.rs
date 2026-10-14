//! Class table columns and the rules that read them.
//!
//! * The outline read the class table; the class root's own `data.progression` was written by
//!   the generation step, which may not have seen the table and guesses. An outline column wins
//!   over a model column of the same key unless both say the same (the model's form is kept
//!   then: "1st" and 1 are the same value).
//! * Every `max {type: table, key}` must name a progression key of the element's class (the
//!   engine reads 0 otherwise): a key written as the column's name is mapped to the key, a key
//!   naming no column drops the rule. Both are reported.
//! * Two ungated `resource` rules with one id: the engine keeps the first, so later ones are
//!   dropped (reported); a table counter is preferred over a fixed or formula one.

use borealis_content::ContentStore;
use serde_json::{json, Map, Value};

use super::{norm, str_at, Derived};
use crate::import::anything::feature_spells::columns;
use crate::import::anything::types::ImportPlan;

/// `data.progression` key form of a column or resource name: "Moss Points" -> "moss_points".
pub(super) fn key_of(name: &str) -> String {
    norm(&name.replace(['_', '-'], " ")).replace(' ', "_")
}

/// The leading integer of a cell ("2nd" -> 2, "+10 ft." -> 10, 3 -> 3), else the trimmed text.
fn loose(v: &Value) -> String {
    let t = match v {
        Value::String(s) => s.trim().to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    if t.is_empty() || t.chars().all(|c| matches!(c, '-' | '—' | '–')) {
        return "0".to_string();
    }
    let digits: String = t
        .trim_start_matches('+')
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let rest = &t.trim_start_matches('+')[digits.len()..];
    let ordinal = ["st", "nd", "rd", "th"].iter().any(|s| rest.trim() == *s);
    match digits.parse::<u64>() {
        Ok(n) if rest.trim().is_empty() || ordinal || rest.starts_with(' ') => n.to_string(),
        _ => t.to_lowercase(),
    }
}

fn same_column(a: &Value, b: &[Value]) -> bool {
    a.as_array()
        .is_some_and(|a| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| loose(x) == loose(y)))
}

/// Outline class-table columns over the model's columns of the same key.
pub(super) fn outline_columns(plan: &ImportPlan, d: &mut Derived) {
    let mut notes = Vec::new();
    for g in &plan.groups {
        let (Some(root), Some(o)) = (g.root_id.as_deref(), g.outline.as_ref()) else {
            continue;
        };
        let cols = columns(&o.columns);
        let Some(prog) = d
            .docs
            .iter_mut()
            .find(|e| str_at(e, "id") == root && str_at(e, "type") == "Class")
            .and_then(|c| c.pointer_mut("/data/progression"))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        for (key, values) in cols {
            match prog.get(&key) {
                Some(model) if same_column(model, &values) => {}
                Some(_) => {
                    prog.insert(key.clone(), json!(values));
                    notes.push((root.to_string(), format!("class table column \"{key}\" taken from the class table (the generated values differed)")));
                }
                None => {
                    prog.insert(key, json!(values));
                }
            }
        }
    }
    d.notes.extend(notes);
}

/// Progression keys of the class called `class`: the draft's own, else the library's.
fn class_keys(class: &str, docs: &[Value], store: &ContentStore) -> Option<Vec<String>> {
    let want = norm(class);
    if want.is_empty() {
        return None;
    }
    let keys = |p: Option<&Map<String, Value>>| p.map(|p| p.keys().cloned().collect());
    if let Some(c) = docs
        .iter()
        .find(|e| str_at(e, "type") == "Class" && norm(str_at(e, "name")) == want)
    {
        return keys(c.pointer("/data/progression").and_then(Value::as_object));
    }
    store
        .by_type("Class")
        .find(|c| norm(&c.element.name) == want)
        .and_then(|c| keys(c.element.data.get("progression").and_then(Value::as_object)))
}

/// The class whose table the element's rules read (the engine: `data.class`; a class itself).
fn class_of(doc: &Value) -> String {
    if str_at(doc, "type") == "Class" {
        return str_at(doc, "name").to_string();
    }
    doc.pointer("/data/class")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

enum KeyFix {
    Keep,
    Map(String),
    Drop,
}

fn fix_key(rule: &Value, keys: &[String]) -> KeyFix {
    let key = rule
        .pointer("/max/key")
        .and_then(Value::as_str)
        .unwrap_or("");
    if keys.iter().any(|k| k == key) {
        return KeyFix::Keep;
    }
    let names = [key, str_at(rule, "name"), str_at(rule, "id")];
    names
        .iter()
        .map(|n| key_of(n))
        .filter(|n| !n.is_empty())
        .find_map(|n| keys.iter().find(|k| key_of(k) == n).cloned())
        .map_or(KeyFix::Drop, KeyFix::Map)
}

fn is_table_max(r: &Value) -> bool {
    r.pointer("/max/type").and_then(Value::as_str) == Some("table")
}

/// Every `max {type: table}` names a progression key of its class, or the rule goes.
pub(super) fn table_keys(store: &ContentStore, d: &mut Derived) {
    let snapshot = d.docs.clone();
    let mut notes = Vec::new();
    for doc in &mut d.docs {
        let id = str_at(doc, "id").to_string();
        let class = class_of(doc);
        let Some(keys) = class_keys(&class, &snapshot, store) else {
            continue;
        };
        for ptr in ["/rules", "/multiclass/rules"] {
            let Some(rules) = doc.pointer_mut(ptr).and_then(Value::as_array_mut) else {
                continue;
            };
            let mut keep = Vec::with_capacity(rules.len());
            for mut r in std::mem::take(rules) {
                if !is_table_max(&r) {
                    keep.push(r);
                    continue;
                }
                let key = r
                    .pointer("/max/key")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                match fix_key(&r, &keys) {
                    KeyFix::Keep => keep.push(r),
                    KeyFix::Map(k) => {
                        notes.push((id.clone(), format!("table key \"{key}\" read as the {class} table column \"{k}\"")));
                        r["max"]["key"] = json!(k);
                        keep.push(r);
                    }
                    KeyFix::Drop => notes.push((id.clone(), format!("warning: rule \"{}\" removed: its table key \"{key}\" is no column of the {class} table (it would count 0)", str_at(&r, "name")))),
                }
            }
            *rules = keep;
        }
    }
    d.notes.extend(notes);
}

fn gated(r: &Value) -> bool {
    ["level", "requirements", "replaces"].iter().any(|k| {
        r.get(*k)
            .is_some_and(|v| !v.is_null() && v != &json!(false))
    })
}

/// `(class + id, doc, rule)` positions of the ungated `resource` rules of class features, in
/// draft order. Only a class's own features are always live together (subclass features,
/// items and species of one id exclude each other, so their twins are not dead).
fn resource_slots(docs: &[Value]) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        let class = norm(&class_of(doc));
        if str_at(doc, "type") != "Class Feature" || class.is_empty() {
            continue;
        }
        let Some(rules) = doc.get("rules").and_then(Value::as_array) else {
            continue;
        };
        for (j, r) in rules.iter().enumerate() {
            let rid = str_at(r, "id");
            if str_at(r, "kind") == "resource" && !rid.is_empty() && !gated(r) {
                out.push((format!("{class}\u{1f}{rid}"), i, j));
            }
        }
    }
    out
}

/// Later ungated duplicates of a resource id among one class's features are dead (the engine
/// keeps the first): drop them, keeping a table counter over a fixed or formula one.
pub(super) fn dedupe_resources(d: &mut Derived) {
    let slots = resource_slots(&d.docs);
    let mut drop: Vec<(usize, usize)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (rid, ..) in &slots {
        if seen.contains(&rid.as_str()) {
            continue;
        }
        seen.push(rid);
        let same: Vec<(usize, usize)> = slots
            .iter()
            .filter(|(r, ..)| r == rid)
            .map(|(_, i, j)| (*i, *j))
            .collect();
        if same.len() < 2 {
            continue;
        }
        let keep = same
            .iter()
            .copied()
            .find(|(i, j)| is_table_max(&d.docs[*i]["rules"][*j]))
            .unwrap_or(same[0]);
        drop.extend(same.into_iter().filter(|s| *s != keep));
    }
    drop.sort_unstable_by(|a, b| b.cmp(a));
    for (i, j) in drop {
        let id = str_at(&d.docs[i], "id").to_string();
        if let Some(rules) = d.docs[i].get_mut("rules").and_then(Value::as_array_mut) {
            let r = rules.remove(j);
            d.notes.push((
                id,
                format!(
                    "duplicate resource \"{}\" removed (another rule already defines it)",
                    str_at(&r, "id")
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ContentStore {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        borealis_content::ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .expect("bundled packs")
            .build()
    }

    fn twenty(f: impl Fn(usize) -> Value) -> Vec<Value> {
        (1..=20).map(f).collect()
    }

    /// The model guessed one column and keyed its own counter by the column's printed name; the
    /// outline column wins, the key is mapped, no twin counter appears, and a counter keyed by
    /// nothing in the table is removed.
    #[test]
    fn outline_columns_win_and_table_keys_resolve() {
        let pool = twenty(|l| json!(l));
        let guessed = twenty(|_| json!(1));
        let ordinal = twenty(|l| json!(format!("{}{}", l.div_ceil(2), "th")));
        let plan: ImportPlan = serde_json::from_value(json!({"edition": "2014", "elements": [],
            "notes": [], "groups": [{"kind": "class", "name": "Tidecaller",
            "root_id": "ID_T_CLASS_TIDECALLER", "outline": {"name": "Tidecaller", "features": [],
            "columns": [{"name": "Moss Points", "values": pool},
                        {"name": "Ward Die", "values": twenty(|_| json!("1d6"))},
                        {"name": "Tide Level", "values": ordinal}]}}]}))
        .expect("plan");
        let numbers = twenty(|l| json!(l.div_ceil(2)));
        let class = json!({"id": "ID_T_CLASS_TIDECALLER", "name": "Tidecaller", "type": "Class",
            "data": {"progression": {"moss_points": guessed, "tide_level": numbers.clone()}}});
        let feature = |id: &str, name: &str, rule: Value| {
            json!({"id": id, "name": name,
            "type": "Class Feature", "data": {"class": "Tidecaller", "level": 1}, "rules": [rule]})
        };
        let docs = vec![
            class,
            feature(
                "ID_T_F_MOSS",
                "Moss Points",
                json!({"kind": "resource", "id": "moss points",
                "name": "Moss Points", "max": {"type": "table", "key": "moss points"}}),
            ),
            feature(
                "ID_T_F_ODD",
                "Odd Gift",
                json!({"kind": "resource", "id": "odd gift",
                "name": "Odd Gift", "max": {"type": "table", "key": "gift_uses"}}),
            ),
        ];
        let d = super::super::derive(&plan, &docs, &store());
        let prog = &d.docs[0]["data"]["progression"];
        assert_eq!(prog["moss_points"][4], json!(5), "outline column wins");
        assert_eq!(
            prog["tide_level"],
            json!(numbers),
            "same values: model form kept"
        );
        assert_eq!(prog["ward_die"][0], json!("1d6"), "missing column added");
        let moss: Vec<&Value> = d
            .docs
            .iter()
            .flat_map(|e| e["rules"].as_array().into_iter().flatten())
            .filter(|r| r["id"] == "moss points")
            .collect();
        assert_eq!(moss.len(), 1, "{moss:?}");
        assert_eq!(moss[0]["max"]["key"], "moss_points");
        assert!(d.docs[2]["rules"].as_array().unwrap().is_empty());
        assert!(
            d.notes.iter().any(|(_, m)| m.contains("gift_uses")),
            "{:?}",
            d.notes
        );
    }

    /// A column no rule reads becomes a counter; when the model already wrote a resource of the
    /// counter's id on the class's features, that rule reads the column instead of a twin.
    #[test]
    fn a_counter_repairs_the_models_resource_of_its_id() {
        let plan: ImportPlan = serde_json::from_value(
            json!({"edition": "2014", "elements": [], "notes": [], "groups": []}),
        )
        .expect("plan");
        let docs = vec![
            json!({"id": "C", "name": "Tidecaller", "type": "Class",
                "data": {"progression": {"glow_points": twenty(|l| json!(l))}}}),
            json!({"id": "F", "name": "Glow", "type": "Class Feature",
                "data": {"class": "Tidecaller", "level": 1}, "rules": [{"kind": "resource",
                "id": "glow points", "name": "Glow Points", "max": {"type": "fixed", "value": 2}}]}),
        ];
        let d = super::super::derive(&plan, &docs, &store());
        let rules = d.docs[1]["rules"].as_array().expect("rules");
        assert_eq!(rules.len(), 1, "{rules:?}");
        assert_eq!(
            rules[0]["max"],
            json!({"type": "table", "key": "glow_points"})
        );
    }

    #[test]
    fn ungated_duplicate_resources_keep_the_table_one() {
        let mut d = Derived {
            docs: vec![
                json!({"id": "A", "type": "Class Feature", "data": {"class": "Tidecaller"},
                    "rules": [{"kind": "resource", "id": "glow",
                    "max": {"type": "fixed", "value": 2}}]}),
                json!({"id": "B", "type": "Class Feature", "data": {"class": "Tidecaller"},
                    "rules": [{"kind": "resource", "id": "glow",
                    "max": {"type": "table", "key": "glow"}},
                    {"kind": "resource", "id": "glow", "level": 5,
                    "max": {"type": "fixed", "value": 3}, "replaces": true}]}),
                // Variants of an item (or features of different subclasses) are never live
                // together: their twins stay.
                json!({"id": "C", "type": "Magic Item", "rules": [{"kind": "resource",
                    "id": "glow", "max": {"type": "fixed", "value": 1}}]}),
                json!({"id": "D", "type": "Archetype Feature", "data": {"class": "Tidecaller"},
                    "rules": [{"kind": "resource", "id": "glow",
                    "max": {"type": "fixed", "value": 1}}]}),
            ],
            ..Derived::default()
        };
        dedupe_resources(&mut d);
        assert!(d.docs[0]["rules"].as_array().unwrap().is_empty());
        assert_eq!(d.docs[1]["rules"].as_array().unwrap().len(), 2);
        assert_eq!(d.docs[2]["rules"].as_array().unwrap().len(), 1);
        assert_eq!(d.docs[3]["rules"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn cells_compare_by_their_number() {
        assert_eq!(loose(&json!("2nd")), "2");
        assert_eq!(loose(&json!(2)), "2");
        assert_eq!(loose(&json!("—")), "0");
        assert_eq!(loose(&json!("+10 ft.")), "10");
        assert_eq!(loose(&json!("1d6")), "1d6");
        assert_eq!(key_of("Moss Points"), "moss_points");
        assert_eq!(key_of("moss-points"), "moss_points");
    }

    #[test]
    fn table_keys_map_by_name_or_drop() {
        let keys = vec!["moss_points".to_string(), "ward_die".to_string()];
        let r = json!({"kind": "resource", "id": "moss points", "name": "Moss Points",
            "max": {"type": "table", "key": "moss points"}});
        assert!(matches!(fix_key(&r, &keys), KeyFix::Map(k) if k == "moss_points"));
        let by_name = json!({"kind": "resource", "id": "x", "name": "Ward Die",
            "max": {"type": "table", "key": "guess"}});
        assert!(matches!(fix_key(&by_name, &keys), KeyFix::Map(k) if k == "ward_die"));
        let bad = json!({"kind": "resource", "id": "x", "name": "Other",
            "max": {"type": "table", "key": "guess"}});
        assert!(matches!(fix_key(&bad, &keys), KeyFix::Drop));
    }
}
