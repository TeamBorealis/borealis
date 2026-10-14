//! Where the source prints a weapon, for a weapon group a class names but the weapons' own data
//! leaves out ("Weapons: ..., <group>" next to weapons the source lists under a standard
//! category): the table lines above the weapon's row, the heading over its table, and the
//! heading over each of its properties' "Name." definitions. A group whose name is in that
//! context holds the weapon.
//!
//! Also reports `proficiency_filter` rules no weapon of the library or the import matches.

use borealis_content::ContentStore;
use serde_json::Value;

use super::{norm, str_at, Derived};
use crate::import::anything::items::{known_property, property_key, weapon_properties};
use crate::import::anything::source::table_head;

/// Lowercase words, each made singular ("-ies" -> "-y", a trailing `s` dropped), space-padded: " firearm property ".
pub(super) fn key_words(text: &str) -> String {
    let words: Vec<String> = norm(text)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            if w.len() > 4 && w.ends_with("ies") {
                format!("{}y", &w[..w.len() - 3])
            } else if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
                w[..w.len() - 1].to_string()
            } else {
                w.to_string()
            }
        })
        .collect();
    format!(" {} ", words.join(" "))
}

fn is_heading(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

fn is_row(line: &str) -> bool {
    line.contains(" | ")
}

/// The nearest heading at or above line `from` (0-based).
fn heading_above(lines: &[&str], from: usize) -> Option<String> {
    (0..=from.min(lines.len().saturating_sub(1)))
        .rev()
        .find(|&k| is_heading(lines[k]))
        .map(|k| lines[k].trim_start_matches('#').trim().to_string())
}

/// The line defining property `name` as a run-in ("Name. What it does."), never a table row.
fn definition_line(lines: &[&str], name: &str) -> Option<usize> {
    let lead = format!("{}.", property_key(name));
    if lead.len() < 3 {
        return None;
    }
    lines.iter().position(|l| {
        let t = l.trim().to_lowercase();
        !is_row(l) && t.starts_with(&lead) && t.len() > lead.len()
    })
}

/// The source text around the weapon whose row is line `row` (0-based): the table's non-row
/// lines above it (caption, category lines), the heading over the table, and the headings over
/// the source's definitions of those of `properties` core does not define (a core property's
/// rules paragraph can sit under any group's heading).
pub(super) fn weapon_context(
    lines: &[&str],
    row: Option<usize>,
    properties: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(row) = row.filter(|&r| r < lines.len()) {
        let top = table_head(lines, row).map_or(row, |(start, _)| start);
        out.extend(
            lines[top..row]
                .iter()
                .filter(|l| !is_row(l) && !l.trim().is_empty())
                .map(|l| l.trim_start_matches('#').trim().to_string()),
        );
        out.extend(heading_above(lines, top));
    }
    for p in properties.iter().filter(|p| known_property(p).is_none()) {
        if let Some(at) = definition_line(lines, p) {
            out.extend(heading_above(lines, at));
        }
    }
    out
}

/// Some context line names the group of key `key` ("firearm" in "Firearm Properties").
pub(super) fn names_group(context: &[String], key: &str) -> bool {
    let needle = format!(" {key} ");
    context.iter().any(|c| key_words(c).contains(&needle))
}

/// The weapon's names to match a filter on: its printed properties and its tags' names.
fn weapon_names(store: &ContentStore, d: &Derived, doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = weapon_properties(doc)
        .iter()
        .map(|p| property_key(p))
        .collect();
    let tags = doc
        .get("supports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    for tag in tags {
        let local = d
            .extra
            .iter()
            .find(|e| str_at(e, "id") == tag)
            .map(|e| str_at(e, "name").to_string());
        if let Some(name) = local.or_else(|| store.get(tag).map(|s| s.element.name.clone())) {
            out.push(property_key(&name));
        }
    }
    for key in ["weapon_category", "weapon_group"] {
        if let Some(c) = doc.pointer(&format!("/data/{key}")).and_then(Value::as_str) {
            out.push(property_key(c));
        }
    }
    out
}

fn filter_terms(rule: &Value, key: &str) -> Vec<String> {
    match rule.get(key) {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// The filter's terms all find a weapon: a category term and any one property term each name
/// one of the weapon's properties, tags or categories (singular or plural).
fn matches(rule: &Value, names: &[String]) -> bool {
    let has = |term: &str| {
        let k = key_words(term);
        names.iter().any(|n| {
            let n = key_words(n);
            n.contains(&k) || k.contains(&n)
        })
    };
    let category = filter_terms(rule, "weapon_category");
    let any = filter_terms(rule, "properties_any");
    category.iter().all(|c| has(c)) && (any.is_empty() || any.iter().any(|p| has(p)))
}

/// Every `proficiency_filter` rule anywhere in `v` (its rules, its multiclass rules).
fn filters_in<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(o) => {
            if o.get("kind").and_then(Value::as_str) == Some("proficiency_filter") {
                out.push(v);
            }
            o.values().for_each(|x| filters_in(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| filters_in(x, out)),
        _ => {}
    }
}

/// A `proficiency_filter` that no weapon of the library or of the import matches grants
/// nothing: reported on its element.
pub(super) fn unmatched_filters(store: &ContentStore, d: &mut Derived) {
    let mut weapons: Vec<Vec<String>> = d
        .docs
        .iter()
        .filter(|e| str_at(e, "type") == "Weapon")
        .map(|e| weapon_names(store, d, e))
        .collect();
    weapons.extend(store.by_type("Weapon").map(|s| {
        let doc = serde_json::json!({"supports": s.element.supports});
        weapon_names(store, d, &doc)
    }));
    let mut found = Vec::new();
    for (i, doc) in d.docs.iter().enumerate() {
        let mut filters = Vec::new();
        filters_in(doc, &mut filters);
        for r in filters {
            let terms =
                filter_terms(r, "weapon_category").len() + filter_terms(r, "properties_any").len();
            let name = str_at(r, "name").to_string();
            if terms > 0
                && !weapons.iter().any(|w| matches(r, w))
                && !found.contains(&(i, name.clone()))
            {
                found.push((i, name));
            }
        }
    }
    for (i, name) in found {
        let id = str_at(&d.docs[i], "id").to_string();
        d.note(
            &id,
            format!("the proficiency filter \"{name}\" matches no weapon of the library or this import, so it grants nothing; check the weapons' category and properties"),
        );
    }
}

#[cfg(test)]
#[path = "weapon_context_tests.rs"]
mod tests;
