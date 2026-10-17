//! Independent source counters for acceptance stage 1. They do NOT reuse the importers' parsers:
//! Aurora = distinct `<element id type>` per type over every file reached (a later id overrides an
//! earlier one, as Aurora loads), 5etools = entries per top-level array per file (`_meta` and
//! fluff excluded, `_copy` entries counted).

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Final type of every element id, in load order (later files override earlier ones).
#[derive(Default)]
pub struct AuroraCount {
    pub types: BTreeMap<String, String>,
    /// `<element` tags without an id or type (never importable).
    pub malformed: usize,
}

impl AuroraCount {
    pub fn add_file(&mut self, text: &str) {
        let text = strip_comments(text);
        let mut rest = text.as_str();
        while let Some(at) = rest.find("<element") {
            rest = &rest[at + "<element".len()..];
            // `<elements>` is the root, not an element
            if !rest.starts_with(|c: char| c.is_whitespace()) {
                continue;
            }
            let end = rest.find('>').unwrap_or(rest.len());
            let tag = &rest[..end];
            match (attr(tag, "id"), attr(tag, "type")) {
                (Some(id), Some(ty)) if !id.is_empty() && !ty.is_empty() => {
                    self.types.insert(id, ty);
                }
                _ => self.malformed += 1,
            }
        }
    }

    pub fn by_type(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for ty in self.types.values() {
            *out.entry(ty.clone()).or_default() += 1;
        }
        out
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("<!--") {
        out.push_str(&rest[..at]);
        rest = rest[at..].find("-->").map_or("", |e| &rest[at + e + 3..]);
    }
    out.push_str(rest);
    out
}

/// Value of `name="..."` (or single quotes) in a start tag, XML entities left as written.
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().last();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) || !after.starts_with('=') {
            continue;
        }
        let value = after[1..].trim_start();
        let quote = value.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let body = &value[1..];
        return body.find(quote).map(|e| body[..e].trim().to_string());
    }
}

/// Every `*.xml` file under `dir`, sorted by relative path (the folder import's order).
pub fn xml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.retain(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")));
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let hidden = p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if hidden {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// 5etools data files that hold rules (the root `*.json` plus `class/`, `spells/`, `homebrew/`),
/// relative to `data`, sorted. Fluff, Foundry and site files are not rules.
pub fn fivetools_files(data: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for sub in ["", "class", "spells", "homebrew"] {
        let dir = data.join(sub);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let skip = !name.ends_with(".json")
                || name.starts_with("fluff-")
                || name.starts_with("foundry")
                || name.starts_with("index")
                || SITE_FILES.contains(&name.as_str());
            if !skip && e.path().is_file() {
                out.push(if sub.is_empty() {
                    name
                } else {
                    format!("{sub}/{name}")
                });
            }
        }
    }
    out.sort();
    out
}

/// Root files of the 5etools site that are not game data (tools, demos, generators).
const SITE_FILES: &[&str] = &[
    "changelog.json",
    "converter.json",
    "renderdemo.json",
    "makebrew-creature.json",
    "makecards.json",
    "encounterbuilder.json",
    "names.json",
    "life.json",
    "loot.json",
    "msbcr.json",
    "books.json",
    "adventures.json",
];

/// Entries per top-level array over the rules files: `array -> (file -> n)`.
pub fn fivetools_entries(data: &Path) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut out: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for rel in fivetools_files(data) {
        let Ok(text) = std::fs::read_to_string(data.join(&rel)) else {
            continue;
        };
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        for (key, value) in map {
            if key.starts_with('_') || key.ends_with("Fluff") {
                continue;
            }
            if let Value::Array(items) = value {
                // each `_versions` entry is an entity of its own (5etools semantics)
                let n: usize = items
                    .iter()
                    .map(|i| {
                        1 + i
                            .get("_versions")
                            .and_then(Value::as_array)
                            .map_or(0, Vec::len)
                    })
                    .sum();
                *out.entry(key).or_default().entry(rel.clone()).or_default() += n;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aurora_counter_reads_ids_types_and_overrides() {
        let mut c = AuroraCount::default();
        c.add_file(
            "<elements><!-- <element id=\"X\" type=\"Feat\"> -->\
             <element name=\"A\" type=\"Feat\" source=\"S\" id=\"ID_A\">\
             <element type='Spell' id='ID_B' name='B'/><element name=\"broken\"></elements>",
        );
        c.add_file("<elements><element id=\"ID_A\" type=\"Race\" name=\"A\"/></elements>");
        assert_eq!(c.types.len(), 2);
        assert_eq!(c.types["ID_A"], "Race");
        assert_eq!(c.malformed, 1);
        assert_eq!(c.by_type()["Spell"], 1);
    }
}
