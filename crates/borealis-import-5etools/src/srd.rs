//! Index of the bundled SRD packs, used to link imported entities to SRD elements.

use borealis_content::{ContentFile, Ruleset};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Coarse entity kinds used for matching names across packs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SrdKind {
    /// Race or species.
    Race,
    /// Subrace.
    Subrace,
    /// Class.
    Class,
    /// Subclass.
    Subclass,
    /// Spell.
    Spell,
    /// Weapon, armor, item or magic item.
    Item,
    /// Feat.
    Feat,
    /// Background.
    Background,
    /// Language.
    Language,
}

impl SrdKind {
    /// The kind for an borealis-content element type string.
    #[must_use]
    pub fn of_type(ty: &str) -> Option<Self> {
        Some(match ty {
            "Race" | "Species" => Self::Race,
            "Sub Race" => Self::Subrace,
            "Class" => Self::Class,
            "Archetype" => Self::Subclass,
            "Spell" => Self::Spell,
            "Weapon" | "Armor" | "Item" | "Magic Item" => Self::Item,
            "Feat" => Self::Feat,
            "Background" => Self::Background,
            "Language" => Self::Language,
            _ => return None,
        })
    }
}

fn normalize(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Names of SRD elements by `(kind, ruleset, normalised name)`.
#[derive(Debug, Default, Clone)]
pub struct SrdIndex {
    map: BTreeMap<(SrdKind, Ruleset, String), String>,
}

impl SrdIndex {
    /// An empty index (no SRD packs configured).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add one element.
    pub fn insert(&mut self, kind: SrdKind, ruleset: Ruleset, name: &str, id: &str) {
        self.map
            .entry((kind, ruleset, normalize(name)))
            .or_insert_with(|| id.to_string());
    }

    /// Look up an SRD element id.
    #[must_use]
    pub fn find(&self, kind: SrdKind, ruleset: Ruleset, name: &str) -> Option<&str> {
        self.map
            .get(&(kind, ruleset, normalize(name)))
            .map(String::as_str)
    }

    /// Number of indexed elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True if nothing is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Index every content JSON file below the directories. Unreadable files are skipped.
    #[must_use]
    pub fn load<P: AsRef<Path>>(dirs: &[P]) -> Self {
        let mut index = Self::default();
        for dir in dirs {
            index.load_dir(dir.as_ref(), None);
        }
        index
    }

    fn load_dir(&mut self, dir: &Path, inherited: Option<Ruleset>) {
        let pack_ruleset = std::fs::read_to_string(dir.join("pack.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| {
                let list = v.get("rulesets")?.as_array()?;
                if list.len() == 1 {
                    serde_json::from_value::<Ruleset>(list[0].clone()).ok()
                } else {
                    None
                }
            })
            .or(inherited);
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<_> = read.filter_map(Result::ok).map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                self.load_dir(&path, pack_ruleset);
            } else if path.extension().is_some_and(|e| e == "json")
                && path.file_name().is_some_and(|n| n != "pack.json")
            {
                self.load_file(&path, pack_ruleset);
            }
        }
    }

    fn load_file(&mut self, path: &Path, pack_ruleset: Option<Ruleset>) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(file) = ContentFile::from_json_str(&text) else {
            return;
        };
        for e in &file.elements {
            let ruleset = e.ruleset.or(file.ruleset).or(pack_ruleset);
            let (Some(kind), Some(ruleset)) = (SrdKind::of_type(&e.ty), ruleset) else {
                continue;
            };
            self.insert(kind, ruleset, &e.name, &e.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ignore_case_and_punctuation() {
        let mut idx = SrdIndex::empty();
        assert!(idx.is_empty());
        idx.insert(
            SrdKind::Spell,
            Ruleset::Dnd2014,
            "Hunter’s Mark",
            "ID_PHB_SPELL_HUNTERS_MARK",
        );
        assert_eq!(
            idx.find(SrdKind::Spell, Ruleset::Dnd2014, "hunter's mark"),
            Some("ID_PHB_SPELL_HUNTERS_MARK")
        );
        assert_eq!(
            idx.find(SrdKind::Spell, Ruleset::Dnd2024, "hunter's mark"),
            None
        );
        assert_eq!(
            idx.find(SrdKind::Item, Ruleset::Dnd2014, "hunter's mark"),
            None
        );
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn kinds() {
        assert_eq!(SrdKind::of_type("Archetype"), Some(SrdKind::Subclass));
        assert_eq!(SrdKind::of_type("Magic Item"), Some(SrdKind::Item));
        assert_eq!(SrdKind::of_type("Species"), Some(SrdKind::Race));
        assert_eq!(SrdKind::of_type("Condition"), None);
    }

    #[test]
    fn loads_pack_dirs_with_ruleset_inheritance() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pack.json"),
            r#"{"format":"borealis-pack/1","id":"x","name":"X","version":"1","rulesets":["dnd2014"],"files":[]}"#,
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(
            dir.path().join("sub/a.json"),
            r#"{"format":"borealis-content/1","elements":[
              {"id":"ID_A","name":"Alpha","type":"Spell","source":"S"},
              {"id":"ID_B","name":"Beta","type":"Spell","source":"S","ruleset":"dnd2024"},
              {"id":"ID_C","name":"Gamma","type":"Condition","source":"S"}]}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("bad.json"), "nope").unwrap();
        let idx = SrdIndex::load(&[dir.path()]);
        assert_eq!(
            idx.find(SrdKind::Spell, Ruleset::Dnd2014, "Alpha"),
            Some("ID_A")
        );
        assert_eq!(
            idx.find(SrdKind::Spell, Ruleset::Dnd2024, "Beta"),
            Some("ID_B")
        );
        assert_eq!(idx.len(), 2);
        let none = SrdIndex::load::<&Path>(&[]);
        assert!(none.is_empty());
    }
}
