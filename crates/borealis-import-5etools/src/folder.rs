//! Reading a 5etools `data/` folder.

use crate::error::ImportError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A raw entry together with the file it came from.
#[derive(Debug, Clone)]
pub struct RawEntry {
    /// Path relative to the data folder, `/` separated.
    pub file: String,
    /// The entry as in the file (an unflagged entry of a book published since the 2024
    /// Player's Handbook gains `edition: "one"`, [`crate::editions`]).
    pub value: Value,
}

/// Everything read from a data folder, not yet resolved or converted.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Entries by property name (`race`, `subrace`, `class`, `spell`, ...).
    pub props: BTreeMap<String, Vec<RawEntry>>,
    /// Fluff entries by the property they describe (`race`, `background`, ...).
    pub fluff: BTreeMap<String, Vec<RawEntry>>,
    /// The spell source lookup (`generated/gendata-spell-source-lookup.json`).
    pub spell_lookup: Option<Value>,
    /// Book names by source id from `books.json`.
    pub books: BTreeMap<String, String>,
    /// Catalog of each source by upper-case id: `book` (`books.json`), `adventure`
    /// (`adventures.json`) or `homebrew` (a homebrew file's `_meta`).
    pub categories: BTreeMap<String, String>,
    /// Publication date (`YYYY-MM-DD`) of each source by upper-case id, from the `published`
    /// field of `books.json` / `adventures.json` (IM8, [`crate::editions`]).
    pub published: BTreeMap<String, String>,
    /// Relative paths of every file that was read, sorted.
    pub files: Vec<String>,
    /// Files that could not be read or parsed: `(path, reason)`.
    pub failed: Vec<(String, String)>,
}

/// Properties read from data files.
const PROPS: &[&str] = &[
    "race",
    "subrace",
    "background",
    "feat",
    "optionalfeature",
    "baseitem",
    "item",
    "itemGroup",
    "magicvariant",
    "itemProperty",
    "itemType",
    "itemMastery",
    "itemEntry",
    "language",
    "skill",
    "sense",
    "condition",
    "disease",
    "status",
    "class",
    "subclass",
    "classFeature",
    "subclassFeature",
    "spell",
    "charoption",
    "monster",
    "monsterTemplate",
    "legendaryGroup",
    "legendaryGroupTemplate",
];

/// Fluff properties and the entity property they belong to.
const FLUFF_PROPS: &[(&str, &str)] = &[
    ("raceFluff", "race"),
    ("backgroundFluff", "background"),
    ("featFluff", "feat"),
    ("itemFluff", "item"),
    ("languageFluff", "language"),
    ("optionalfeatureFluff", "optionalfeature"),
    ("conditionFluff", "condition"),
    ("classFluff", "class"),
    ("subclassFluff", "subclass"),
    ("spellFluff", "spell"),
];

/// Files the importer reads from the data folder by fixed path, besides the `class/`,
/// `spells/` and `bestiary/` files it lists, the `homebrew/` files and [`SPELL_SOURCE_LOOKUP`].
/// A URL import downloads exactly these (src-tauri `content_fivetools_remote`).
pub const ROOT_FILES: &[&str] = &[
    "races.json",
    "backgrounds.json",
    "feats.json",
    "optionalfeatures.json",
    "items-base.json",
    "items.json",
    "magicvariants.json",
    "languages.json",
    "skills.json",
    "senses.json",
    "conditionsdiseases.json",
    "charcreationoptions.json",
    "books.json",
    "adventures.json",
    "fluff-races.json",
    "fluff-backgrounds.json",
    "fluff-feats.json",
    "fluff-items.json",
    "fluff-languages.json",
    "fluff-optionalfeatures.json",
    "fluff-conditionsdiseases.json",
    "bestiary/template.json",
    "bestiary/legendarygroups.json",
];

/// The spell source lookup the importer reads when the data folder has it.
pub const SPELL_SOURCE_LOOKUP: &str = "generated/gendata-spell-source-lookup.json";

/// Sub-folder of the data folder whose `*.json` files are read as homebrew.
pub const HOMEBREW_DIR: &str = "homebrew";

/// Relative paths (sorted) of the data files the importer reads.
///
/// # Errors
/// [`ImportError::NotADirectory`] when `root` is not a directory.
pub fn data_files(root: &Path) -> Result<Vec<String>, ImportError> {
    if !root.is_dir() {
        return Err(ImportError::NotADirectory(root.to_path_buf()));
    }
    let mut out = Vec::new();
    for name in ROOT_FILES {
        if root.join(name).is_file() {
            out.push((*name).to_string());
        }
    }
    for (dir, prefixes) in [
        ("class", &["class-", "fluff-class-"][..]),
        ("spells", &["spells-", "fluff-spells-"][..]),
        // creatures (`bestiary/index.json` lists the same files); their fluff is not read
        ("bestiary", &["bestiary-"][..]),
    ] {
        let dir_path = root.join(dir);
        let Ok(read) = std::fs::read_dir(&dir_path) else {
            continue;
        };
        let mut names: Vec<String> = read
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json") && prefixes.iter().any(|p| n.starts_with(p)))
            .collect();
        names.sort();
        out.extend(names.into_iter().map(|n| format!("{dir}/{n}")));
    }
    // Homebrew files (5etools' homebrew format, `_meta` + entity arrays) dropped into a
    // `homebrew/` folder are read like the official ones, so their `_copy`s resolve.
    if let Ok(read) = std::fs::read_dir(root.join(HOMEBREW_DIR)) {
        let mut names: Vec<String> = read
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json"))
            .collect();
        names.sort();
        out.extend(names.into_iter().map(|n| format!("{HOMEBREW_DIR}/{n}")));
    }
    if root.join(SPELL_SOURCE_LOOKUP).is_file() {
        out.push(SPELL_SOURCE_LOOKUP.to_string());
    }
    out.sort();
    Ok(out)
}

/// Largest data file read from a folder.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

fn read_json(path: &Path) -> Result<Value, ImportError> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| ImportError::io(path, e))?;
    let refuse = |msg: &str| {
        ImportError::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string()),
        )
    };
    if meta.file_type().is_symlink() {
        return Err(refuse("skipped: symbolic links are not followed"));
    }
    if !meta.is_file() {
        return Err(refuse("skipped: not a regular file"));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(refuse("skipped: file is larger than 64 MiB"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| ImportError::io(path, e))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str(text).map_err(|e| {
        ImportError::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
        )
    })
}

/// Read every data file of the folder.
///
/// # Errors
/// [`ImportError::NotADirectory`] / [`ImportError::NotFiveTools`]. Individual unreadable
/// files are recorded in [`Loaded::failed`] instead.
pub fn load(
    root: &Path,
    mut on_file: impl FnMut(usize, usize, &str),
) -> Result<Loaded, ImportError> {
    let files = data_files(root)?;
    if files.is_empty() {
        return Err(ImportError::NotFiveTools(root.to_path_buf()));
    }
    let mut loaded = Loaded::default();
    let total = files.len();
    for (i, rel) in files.iter().enumerate() {
        on_file(i + 1, total, rel);
        let path: PathBuf = root.join(rel);
        let value = match read_json(&path) {
            Ok(v) => v,
            Err(e) => {
                loaded.failed.push((rel.clone(), e.to_string()));
                continue;
            }
        };
        loaded.files.push(rel.clone());
        absorb(&mut loaded, rel, value);
    }
    crate::editions::mark_late_entries(&mut loaded);
    Ok(loaded)
}

fn absorb(loaded: &mut Loaded, rel: &str, value: Value) {
    if rel == SPELL_SOURCE_LOOKUP {
        loaded.spell_lookup = Some(value);
        return;
    }
    let Value::Object(mut map) = value else {
        loaded
            .failed
            .push((rel.to_string(), "top level is not an object".to_string()));
        return;
    };
    if let Some(meta) = map.remove("_meta") {
        // homebrew: `_meta.sources[] = {json: "ABBR", full: "Name"}`
        for s in meta
            .get("sources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let abbr = s.get("json").and_then(Value::as_str);
            let name = s.get("full").and_then(Value::as_str);
            if let (Some(abbr), Some(name)) = (abbr, name) {
                loaded
                    .books
                    .entry(abbr.to_uppercase())
                    .or_insert_with(|| name.to_string());
                loaded
                    .categories
                    .entry(abbr.to_uppercase())
                    .or_insert_with(|| "homebrew".to_string());
            }
        }
    }
    let catalog = match rel {
        "books.json" => Some("book"),
        "adventures.json" => Some("adventure"),
        _ => None,
    };
    if let Some(prop) = catalog {
        if let Some(Value::Array(books)) = map.remove(prop) {
            for b in books {
                let id = b
                    .get("source")
                    .or_else(|| b.get("id"))
                    .and_then(Value::as_str);
                let name = b.get("name").and_then(Value::as_str);
                if let (Some(id), Some(date)) = (id, b.get("published").and_then(Value::as_str)) {
                    loaded.published.insert(id.to_uppercase(), date.to_string());
                }
                if let (Some(id), Some(name)) = (id, name) {
                    loaded.books.insert(id.to_uppercase(), name.to_string());
                    loaded
                        .categories
                        .insert(id.to_uppercase(), prop.to_string());
                }
            }
        }
        return;
    }
    for prop in PROPS {
        if let Some(Value::Array(items)) = map.remove(*prop) {
            let entries = loaded.props.entry((*prop).to_string()).or_default();
            entries.extend(items.into_iter().map(|value| RawEntry {
                file: rel.to_string(),
                value,
            }));
        }
    }
    for (fluff_prop, target) in FLUFF_PROPS {
        if let Some(Value::Array(items)) = map.remove(*fluff_prop) {
            let entries = loaded.fluff.entry((*target).to_string()).or_default();
            entries.extend(items.into_iter().map(|value| RawEntry {
                file: rel.to_string(),
                value,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write(dir: &Path, rel: &str, value: &Value) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_string(value).unwrap()).unwrap();
    }

    #[test]
    fn homebrew_files_and_catalog_names_are_read() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "races.json",
            &json!({"race": [{"name": "A", "source": "ZZZ"}]}),
        );
        write(
            dir.path(),
            "adventures.json",
            &json!({"adventure": [{"id": "QQA", "source": "QQA", "name": "Quest A"}]}),
        );
        write(
            dir.path(),
            "homebrew/my brew.json",
            &json!({"_meta": {"sources": [{"json": "HBX", "full": "Home Brew X"}]},
                    "race": [{"name": "B", "source": "HBX", "_copy": {"name": "A", "source": "ZZZ"}}]}),
        );
        write(dir.path(), "homebrew/notes.txt", &json!({}));
        let loaded = load(dir.path(), |_, _, _| {}).unwrap();
        assert!(loaded.files.contains(&"homebrew/my brew.json".to_string()));
        assert!(!loaded.files.iter().any(|f| f.ends_with(".txt")));
        assert_eq!(loaded.props["race"].len(), 2);
        assert_eq!(loaded.books["HBX"], "Home Brew X");
        assert_eq!(loaded.books["QQA"], "Quest A");
        assert_eq!(loaded.categories["HBX"], "homebrew");
        assert_eq!(loaded.categories["QQA"], "adventure");
    }

    #[test]
    fn oversized_files_are_skipped_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "races.json", &json!({"race": []}));
        write(dir.path(), "items.json", &json!({"item": []}));
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join("items.json"))
            .unwrap();
        f.set_len(MAX_FILE_BYTES + 1).unwrap();
        let loaded = load(dir.path(), |_, _, _| {}).unwrap();
        assert_eq!(loaded.files, vec!["races.json".to_string()]);
        assert!(loaded
            .failed
            .iter()
            .any(|(n, m)| n == "items.json" && m.contains("64 MiB")));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_files_are_skipped_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(dir.path(), "races.json", &json!({"race": []}));
        write(outside.path(), "secret.json", &json!({"item": []}));
        std::os::unix::fs::symlink(
            outside.path().join("secret.json"),
            dir.path().join("items.json"),
        )
        .unwrap();
        let loaded = load(dir.path(), |_, _, _| {}).unwrap();
        assert_eq!(loaded.files, vec!["races.json".to_string()]);
        assert!(loaded
            .failed
            .iter()
            .any(|(n, m)| n == "items.json" && m.contains("symbolic")));
    }

    #[test]
    fn reads_known_files_in_sorted_order_and_ignores_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "races.json",
            &json!({"race": [{"name": "A", "source": "X"}], "_meta": {}}),
        );
        write(
            dir.path(),
            "class/class-b.json",
            &json!({"class": [{"name": "B"}], "classFeature": []}),
        );
        write(
            dir.path(),
            "class/class-a.json",
            &json!({"class": [{"name": "A"}]}),
        );
        write(
            dir.path(),
            "class/foundry.json",
            &json!({"class": [{"name": "Z"}]}),
        );
        write(
            dir.path(),
            "spells/spells-x.json",
            &json!({"spell": [{"name": "S"}]}),
        );
        write(
            dir.path(),
            "spells/fluff-spells-x.json",
            &json!({"spellFluff": [{"name": "S", "source": "X"}]}),
        );
        write(
            dir.path(),
            "fluff-races.json",
            &json!({"raceFluff": [{"name": "A", "source": "X"}]}),
        );
        write(
            dir.path(),
            "bestiary/bestiary-x.json",
            &json!({"monster": []}),
        );
        write(
            dir.path(),
            "bestiary/fluff-bestiary-x.json",
            &json!({"monsterFluff": []}),
        );
        write(
            dir.path(),
            "bestiary/legendarygroups.json",
            &json!({"legendaryGroup": []}),
        );
        write(
            dir.path(),
            "books.json",
            &json!({"book": [{"id": "X", "name": "Book X", "source": "X"}]}),
        );
        write(
            dir.path(),
            "generated/gendata-spell-source-lookup.json",
            &json!({"x": {}}),
        );
        let files = data_files(dir.path()).unwrap();
        assert_eq!(
            files,
            vec![
                "bestiary/bestiary-x.json",
                "bestiary/legendarygroups.json",
                "books.json",
                "class/class-a.json",
                "class/class-b.json",
                "fluff-races.json",
                "generated/gendata-spell-source-lookup.json",
                "races.json",
                "spells/fluff-spells-x.json",
                "spells/spells-x.json",
            ]
        );
        let mut seen = Vec::new();
        let loaded = load(dir.path(), |i, n, f| seen.push((i, n, f.to_string()))).unwrap();
        assert_eq!(seen.len(), 10);
        assert_eq!(seen[0].1, 10);
        assert!(loaded.props["legendaryGroup"].is_empty());
        assert_eq!(loaded.props["class"].len(), 2);
        assert_eq!(loaded.props["class"][0].value["name"], "A");
        assert_eq!(loaded.props["race"][0].file, "races.json");
        assert_eq!(loaded.fluff["race"].len(), 1);
        assert_eq!(loaded.fluff["spell"].len(), 1);
        assert_eq!(loaded.books["X"], "Book X");
        assert!(loaded.spell_lookup.is_some());
        assert!(loaded.failed.is_empty());
    }

    #[test]
    fn errors_for_wrong_folders_and_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            load(dir.path(), |_, _, _| {}),
            Err(ImportError::NotFiveTools(_))
        ));
        assert!(matches!(
            data_files(&dir.path().join("missing")),
            Err(ImportError::NotADirectory(_))
        ));
        std::fs::write(dir.path().join("races.json"), "{ not json").unwrap();
        write(
            dir.path(),
            "feats.json",
            &json!({"feat": [{"name": "F", "source": "X"}]}),
        );
        let loaded = load(dir.path(), |_, _, _| {}).unwrap();
        assert_eq!(loaded.failed.len(), 1);
        assert_eq!(loaded.failed[0].0, "races.json");
        assert_eq!(loaded.props["feat"].len(), 1);
        // BOM tolerated
        std::fs::write(
            dir.path().join("languages.json"),
            "\u{feff}{\"language\":[]}",
        )
        .unwrap();
        let loaded = load(dir.path(), |_, _, _| {}).unwrap();
        assert_eq!(loaded.failed.len(), 1);
    }
}
