//! Edition by publication date (user decision IM8): an entry of a book or adventure published
//! on or after the 2024 Player's Handbook that carries no `edition` flag is 2024 content
//! (`edition: "one"`). The dates are the `published` fields of the folder's own `books.json`
//! and `adventures.json`; without them nothing changes. A 2014 character still reaches that
//! content through its "allow content from the other edition" setting.

use crate::folder::Loaded;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Source of the 2024 Player's Handbook, whose publication date is the cutoff.
const XPHB: &str = "XPHB";
/// Publication date of the 2024 Player's Handbook, used when `books.json` does not list it.
const XPHB_PUBLISHED: &str = "2024-09-17";
/// The `edition` value of 2024 content.
const EDITION_2024: &str = "one";
/// Property of magic variants, whose produced items take their fields from `inherits`.
const MAGIC_VARIANT: &str = "magicvariant";
/// Marker of a magic variant whose 2024 edition comes from its book's date: it still applies
/// to the 2014 base items that have no 2024 reprint (a hoopak, a yklwa), as it did before.
pub const EDITION_BY_DATE: &str = "_editionByDate";

/// True for an ISO date (`YYYY-MM-DD`), so dates compare as text.
fn is_iso_date(d: &str) -> bool {
    let b = d.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// Upper-case sources published on or after the 2024 Player's Handbook.
#[must_use]
pub fn late_sources(published: &BTreeMap<String, String>) -> BTreeSet<String> {
    let cutoff = published
        .get(XPHB)
        .map(String::as_str)
        .filter(|d| is_iso_date(d))
        .unwrap_or(XPHB_PUBLISHED);
    published
        .iter()
        .filter(|(_, d)| is_iso_date(d) && d.as_str() >= cutoff)
        .map(|(s, _)| s.to_uppercase())
        .collect()
}

fn has_edition(v: &Value) -> bool {
    v.get("edition").and_then(Value::as_str).is_some()
}

/// Mark the unflagged entries of late sources as 2024 content. A magic variant names its
/// source in `inherits`, and the items it produces take their edition from there too.
pub fn mark_late_entries(loaded: &mut Loaded) {
    let late = late_sources(&loaded.published);
    if late.is_empty() {
        return;
    }
    for (prop, entries) in &mut loaded.props {
        let variant = prop == MAGIC_VARIANT;
        for e in entries {
            let Value::Object(map) = &mut e.value else {
                continue;
            };
            let inherits = map.get("inherits");
            let source = map
                .get("source")
                .or_else(|| inherits.and_then(|i| i.get("source")))
                .and_then(Value::as_str)
                .map(str::to_uppercase);
            let flagged = map.get("edition").and_then(Value::as_str).is_some()
                || inherits.is_some_and(has_edition);
            if flagged || !source.is_some_and(|s| late.contains(&s)) {
                continue;
            }
            map.insert("edition".into(), Value::String(EDITION_2024.into()));
            if variant {
                map.insert(EDITION_BY_DATE.into(), Value::Bool(true));
                if let Some(Value::Object(i)) = map.get_mut("inherits") {
                    i.insert("edition".into(), Value::String(EDITION_2024.into()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder::RawEntry;
    use serde_json::json;

    fn dates(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(s, d)| ((*s).to_string(), (*d).to_string()))
            .collect()
    }

    #[test]
    fn the_cutoff_is_the_2024_players_handbook() {
        let late = late_sources(&dates(&[
            ("XPHB", "2024-09-17"),
            ("BEFORE", "2024-09-16"),
            ("SAME", "2024-09-17"),
            ("AFTER", "2026-09-15"),
            ("ODD", "soon"),
        ]));
        assert!(late.contains("SAME") && late.contains("AFTER") && late.contains("XPHB"));
        assert!(!late.contains("BEFORE") && !late.contains("ODD"));
        // without the handbook's own entry the known date is the cutoff
        let late = late_sources(&dates(&[("A", "2024-09-16"), ("B", "2024-09-17")]));
        assert_eq!(late, BTreeSet::from(["B".to_string()]));
    }

    #[test]
    fn unflagged_entries_of_late_books_become_2024() {
        let mut loaded = Loaded {
            published: dates(&[
                ("XPHB", "2024-09-17"),
                ("NEW", "2025-01-01"),
                ("OLD", "2020-01-01"),
            ]),
            ..Loaded::default()
        };
        let raw = |v: Value| RawEntry {
            file: "x.json".into(),
            value: v,
        };
        loaded.props.insert(
            "feat".into(),
            vec![
                raw(json!({"name": "A", "source": "NEW"})),
                raw(json!({"name": "B", "source": "new", "edition": "classic"})),
                raw(json!({"name": "C", "source": "OLD"})),
            ],
        );
        loaded.props.insert(
            "magicvariant".into(),
            vec![raw(json!({"name": "V", "inherits": {"source": "NEW"}}))],
        );
        mark_late_entries(&mut loaded);
        let feats: Vec<Option<&str>> = loaded.props["feat"]
            .iter()
            .map(|e| e.value.get("edition").and_then(Value::as_str))
            .collect();
        assert_eq!(feats, [Some("one"), Some("classic"), None]);
        let v = &loaded.props["magicvariant"][0].value;
        assert_eq!(v["edition"], "one");
        assert_eq!(v["inherits"]["edition"], "one");
        assert_eq!(v[EDITION_BY_DATE], true);
    }
}
