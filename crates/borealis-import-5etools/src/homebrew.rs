//! Single 5etools-format homebrew files: one JSON object with a `_meta` block (its own sources,
//! dependencies) and entity arrays (`spell`, `item`, `monster`, `class`, `subclass`, ...).
//!
//! The folder importer reads such files from a data folder's `homebrew/` sub-folder
//! ([`crate::folder::HOMEBREW_DIR`]); a file picked on its own is staged there by the app. This
//! module recognises the shape and names what a file holds that Borealis does not import, so the
//! report can say why those entries were skipped.

use crate::folder::{FLUFF_PROPS, PROPS};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// True for a top-level property the importer reads (an entity array or a fluff array it uses).
#[must_use]
pub fn is_read_prop(prop: &str) -> bool {
    PROPS.contains(&prop) || FLUFF_PROPS.iter().any(|(f, _)| *f == prop)
}

/// True for a top-level key that is bookkeeping, not content (`_meta`, `_test`, `$schema`).
fn is_bookkeeping(key: &str) -> bool {
    key.starts_with('_') || key.starts_with('$')
}

/// True when `value` is a 5etools-format data or homebrew file: an object with a `_meta` object or
/// at least one entity array the importer reads, and not a Borealis content file (`elements`).
#[must_use]
pub fn looks_like_5etools(value: &Value) -> bool {
    let Value::Object(map) = value else {
        return false;
    };
    if map.contains_key("elements") {
        return false;
    }
    map.get("_meta").is_some_and(Value::is_object)
        || map
            .iter()
            .any(|(k, v)| v.is_array() && PROPS.contains(&k.as_str()))
}

/// Parse `bytes` (UTF-8, a byte order mark tolerated) as JSON; `None` when it is not JSON.
#[must_use]
pub fn parse_json(bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?;
    serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(text)).ok()
}

/// True when `bytes` hold a 5etools-format JSON file ([`looks_like_5etools`]).
#[must_use]
pub fn is_5etools_json(bytes: &[u8]) -> bool {
    parse_json(bytes).is_some_and(|v| looks_like_5etools(&v))
}

/// Number of entries in the entity arrays the importer reads.
#[must_use]
pub fn importable_entries(value: &Value) -> usize {
    value.as_object().map_or(0, |map| {
        map.iter()
            .filter(|(k, _)| PROPS.contains(&k.as_str()))
            .filter_map(|(_, v)| v.as_array())
            .map(Vec::len)
            .sum()
    })
}

/// Entry counts of the arrays the importer does not read, by property (`deity`, `vehicle`, ...).
#[must_use]
pub fn unsupported_kinds(map: &Map<String, Value>) -> BTreeMap<String, usize> {
    map.iter()
        .filter(|(k, _)| !is_bookkeeping(k) && !is_read_prop(k))
        .filter_map(|(k, v)| Some((k.clone(), v.as_array()?.len())))
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// True for an array a homebrew file may hold that the importer does not read.
#[must_use]
pub fn is_unsupported_prop(key: &str) -> bool {
    !is_bookkeeping(key) && !is_read_prop(key)
}

/// What Borealis lacks for entries of `prop`, in words for the import report.
fn missing_label(prop: &str) -> Option<&'static str> {
    Some(match prop {
        "deity" => "deities",
        "vehicle" | "vehicleUpgrade" => "vehicles",
        "object" => "objects",
        "trap" | "hazard" => "traps or hazards",
        "table" | "tableGroup" | "encounter" | "name" => "roll tables",
        "variantrule" => "variant rules",
        "reward" => "supernatural gifts or rewards",
        "boon" | "cult" => "cults or boons",
        "psionic" => "psionics",
        "recipe" => "recipes",
        "deck" | "card" => "card decks",
        "adventure" | "adventureData" | "book" | "bookData" => "book or adventure chapters",
        "action" => "a rules glossary of actions",
        "facility" => "bastion facilities",
        "makebrewCreatureTrait" | "makebrewCreatureAction" => "creature builder presets",
        "monsterFluff" => "lore text for creatures",
        _ => return None,
    })
}

/// The skip reason shown for an entry of `prop` the importer does not read.
#[must_use]
pub fn unsupported_reason(prop: &str) -> String {
    match missing_label(prop) {
        Some(label) => format!("not imported: Borealis has no {label}"),
        None if prop.ends_with("Fluff") => {
            "not imported: lore text for this kind of entry is not read".to_string()
        }
        None => format!("not imported: Borealis has no place for \"{prop}\" entries"),
    }
}

/// The sources a homebrew file's `_meta.sources` declares (`json` ids).
#[must_use]
pub fn declared_sources(meta: &Value) -> Vec<String> {
    meta.get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.get("json").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Sources a homebrew file needs loaded beside it (`_meta.dependencies` and `_meta.includes`:
/// `{ "<kind>": ["<source>", ...] }`).
#[must_use]
pub fn dependencies(meta: &Value) -> BTreeSet<String> {
    ["dependencies", "includes"]
        .iter()
        .filter_map(|k| meta.get(*k).and_then(Value::as_object))
        .flat_map(Map::values)
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recognises_homebrew_and_data_files_but_not_borealis_content() {
        assert!(looks_like_5etools(&json!({"_meta": {"sources": []}})));
        assert!(looks_like_5etools(
            &json!({"spell": [{"name": "A", "source": "X"}]})
        ));
        assert!(!looks_like_5etools(
            &json!({"format": "borealis-content/1", "elements": []})
        ));
        assert!(!looks_like_5etools(&json!({"_meta": {}, "elements": []})));
        assert!(!looks_like_5etools(&json!({"name": "A character"})));
        assert!(!looks_like_5etools(&json!([{"spell": []}])));
        assert!(is_5etools_json("\u{feff}{\"_meta\":{}}".as_bytes()));
        assert!(!is_5etools_json(b"<elements/>"));
    }

    #[test]
    fn names_what_is_not_imported() {
        let v = json!({"_meta": {}, "$schema": "x", "spell": [{}], "deity": [{}, {}],
                       "vehicle": [], "raceFluff": [{}], "monsterFluff": [{}], "zzz": [{}], "note": "x"});
        let kinds = unsupported_kinds(v.as_object().unwrap());
        assert_eq!(
            kinds.into_iter().collect::<Vec<_>>(),
            vec![
                ("deity".to_string(), 2),
                ("monsterFluff".to_string(), 1),
                ("zzz".to_string(), 1)
            ]
        );
        assert_eq!(importable_entries(&v), 1);
        assert_eq!(
            unsupported_reason("deity"),
            "not imported: Borealis has no deities"
        );
        assert!(unsupported_reason("zzz").contains("\"zzz\""));
        assert!(unsupported_reason("objectFluff").contains("lore text"));
        assert!(is_unsupported_prop("deity"));
        assert!(!is_unsupported_prop("subclassFeature"));
        assert!(!is_unsupported_prop("_meta"));
    }

    #[test]
    fn reads_sources_and_dependencies_from_meta() {
        let meta = json!({"sources": [{"json": "QB", "full": "Q"}, {"full": "no id"}],
                          "dependencies": {"subclass": ["OtherBrew"], "spell": ["OtherBrew", ""]},
                          "includes": {"item": ["Extras"]}});
        assert_eq!(declared_sources(&meta), vec!["QB".to_string()]);
        assert_eq!(
            dependencies(&meta).into_iter().collect::<Vec<_>>(),
            vec!["Extras".to_string(), "OtherBrew".to_string()]
        );
        assert!(dependencies(&json!({})).is_empty());
    }
}
