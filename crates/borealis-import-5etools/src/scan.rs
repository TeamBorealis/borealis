//! Read-only inventory of a 5etools data folder (the `scan_5etools_folder` command).

use crate::error::ImportError;
use crate::folder::{self, Loaded};
use crate::source::{default_source_name, ruleset_of_entry};
use borealis_content::Ruleset;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One source (book) found in a data folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSummary {
    /// Source abbreviation as written in the data (`PHB`).
    pub abbr: String,
    /// Display name (`books.json` first, then the built-in table, then the abbreviation).
    pub name: String,
    /// `dnd2014`, `dnd2024` or `mixed`.
    pub ruleset: String,
    /// `book`, `adventure`, `homebrew` (a homebrew file) or `other` (in no catalog).
    pub category: String,
    /// Entry counts by kind (`race`, `class`, `spell`, `item`, ...). Kinds with no entry are absent.
    pub counts: BTreeMap<String, usize>,
}

/// The `FiveToolsScan` DTO of the UI contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FiveToolsScan {
    /// Sources sorted by abbreviation.
    pub sources: Vec<SourceSummary>,
}

/// The kind label a raw property is counted under, if it is counted at all.
fn count_kind(prop: &str) -> Option<&'static str> {
    Some(match prop {
        "race" => "race",
        "subrace" => "subrace",
        "class" => "class",
        "subclass" => "subclass",
        "background" => "background",
        "feat" => "feat",
        "optionalfeature" => "optionalfeature",
        "spell" => "spell",
        "item" | "baseitem" | "itemGroup" | "magicvariant" => "item",
        "language" => "language",
        "condition" | "disease" | "status" => "condition",
        "monster" => "monster",
        _ => return None,
    })
}

#[derive(Default)]
struct Acc {
    counts: BTreeMap<String, usize>,
    rulesets: BTreeSet<Ruleset>,
}

fn summarize(loaded: &Loaded) -> Vec<SourceSummary> {
    // Group case-insensitively but keep the first spelling seen.
    let mut spelling: BTreeMap<String, String> = BTreeMap::new();
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
    for (prop, entries) in &loaded.props {
        let Some(kind) = count_kind(prop) else {
            continue;
        };
        for e in entries {
            let Some(source) = e.value.get("source").and_then(|s| s.as_str()) else {
                continue;
            };
            let upper = source.to_uppercase();
            spelling
                .entry(upper.clone())
                .or_insert_with(|| source.to_string());
            let a = acc.entry(upper).or_default();
            *a.counts.entry(kind.to_string()).or_default() += 1;
            a.rulesets.insert(ruleset_of_entry(&e.value));
        }
    }
    acc.into_iter()
        .map(|(upper, a)| {
            let abbr = spelling.remove(&upper).unwrap_or_else(|| upper.clone());
            let name = loaded
                .books
                .get(&upper)
                .cloned()
                .unwrap_or_else(|| default_source_name(&abbr));
            let ruleset = if a.rulesets.len() == 1 {
                a.rulesets.iter().next().map_or("mixed", |r| r.as_str())
            } else {
                "mixed"
            };
            let category = loaded
                .categories
                .get(&upper)
                .cloned()
                .unwrap_or_else(|| "other".to_string());
            SourceSummary {
                abbr,
                name,
                ruleset: ruleset.to_string(),
                category,
                counts: a.counts,
            }
        })
        .collect()
}

/// List the sources of a 5etools `data/` folder with entry counts. Nothing is converted and
/// nothing is written.
///
/// # Errors
/// [`ImportError::NotADirectory`] / [`ImportError::NotFiveTools`] for a wrong folder.
pub fn scan_folder(path: &Path) -> Result<Vec<SourceSummary>, ImportError> {
    let loaded = folder::load(path, |_, _, _| {})?;
    Ok(summarize(&loaded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write(dir: &Path, rel: &str, v: &serde_json::Value) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v.to_string()).unwrap();
    }

    #[test]
    fn counts_sources_and_rulesets() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "races.json",
            &json!({"race": [{"name": "A", "source": "ZZZ"}, {"name": "B", "source": "zzz"}, {"name": "C", "source": "XPHB"}]}),
        );
        write(
            dir.path(),
            "items-base.json",
            &json!({"baseitem": [{"name": "Rope", "source": "ZZZ"}]}),
        );
        write(
            dir.path(),
            "books.json",
            &json!({"book": [{"id": "ZZZ", "source": "ZZZ", "name": "The Z Book"}]}),
        );
        write(
            dir.path(),
            "class/class-q.json",
            &json!({"classFeature": [{"name": "F", "source": "ZZZ"}]}),
        );
        let s = scan_folder(dir.path()).unwrap();
        let abbrs: Vec<_> = s.iter().map(|x| x.abbr.as_str()).collect();
        assert_eq!(abbrs, vec!["XPHB", "ZZZ"]);
        assert_eq!(s[1].name, "The Z Book");
        assert_eq!(s[1].counts["race"], 2);
        assert_eq!(s[1].counts["item"], 1);
        assert!(!s[1].counts.contains_key("classFeature"));
        assert_eq!(s[1].category, "book");
        assert_eq!(s[0].category, "other");
        assert_eq!(s[0].ruleset, "dnd2024");
        assert_eq!(s[1].ruleset, "dnd2014");
        let json = serde_json::to_value(FiveToolsScan { sources: s }).unwrap();
        assert!(json["sources"][0]["counts"].is_object());
    }

    #[test]
    fn rejects_wrong_folders() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            scan_folder(dir.path()),
            Err(ImportError::NotFiveTools(_))
        ));
        assert!(matches!(
            scan_folder(&dir.path().join("missing")),
            Err(ImportError::NotADirectory(_))
        ));
    }
}
