//! Output packs: grouping converted elements per source and writing them to disk.

use crate::error::ImportError;
use borealis_content::{
    ContentFile, Element, Extra, Info, PackFile, PackFileKind, PackFormat, PackManifest,
    PackOrigin, Ruleset,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Value of the `origin` metadata written to every imported pack.
pub const ORIGIN: &str = "5etools-import";

/// One content file of a pack.
#[derive(Debug, Clone, PartialEq)]
pub struct PackContent {
    /// Path relative to the pack directory.
    pub path: String,
    /// The content.
    pub file: ContentFile,
}

/// A converted pack: all content of one 5etools source.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPack {
    /// Source abbreviation (`PHB`, `XPHB`, ...).
    pub source: String,
    /// Manifest (`pack.json`).
    pub manifest: PackManifest,
    /// Content files in manifest order.
    pub files: Vec<PackContent>,
    /// Always `"5etools-import"`.
    pub origin: &'static str,
    /// Always `false`: imported content must never be redistributed.
    pub redistributable: bool,
    /// Ruleset of the pack, or `None` for a pack mixing both.
    pub ruleset: Option<Ruleset>,
}

impl ImportedPack {
    /// Number of elements over all files.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.files.iter().map(|f| f.file.elements.len()).sum()
    }

    /// All elements in file order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.files.iter().flat_map(|f| f.file.elements.iter())
    }

    /// Directory name of the pack (the manifest id).
    #[must_use]
    pub fn dir_name(&self) -> &str {
        &self.manifest.id
    }

    /// Write `pack.json`, the content files and `import.json` into `<parent>/<id>/`.
    ///
    /// # Errors
    /// I/O and serialisation failures.
    pub fn write_to(&self, parent: &Path) -> Result<PathBuf, ImportError> {
        let dir = parent.join(self.dir_name());
        std::fs::create_dir_all(&dir).map_err(|e| ImportError::io(&dir, e))?;
        for f in &self.files {
            let path = dir.join(&f.path);
            std::fs::write(&path, f.file.to_json_pretty()?)
                .map_err(|e| ImportError::io(&path, e))?;
        }
        let manifest_path = dir.join("pack.json");
        std::fs::write(&manifest_path, self.manifest.to_json_pretty()?)
            .map_err(|e| ImportError::io(&manifest_path, e))?;
        let meta = serde_json::json!({
            "origin": self.origin,
            "redistributable": self.redistributable,
            "source": self.source,
            "ruleset": self.ruleset.map_or("mixed", Ruleset::as_str),
            "importer": concat!("borealis-import-5etools ", env!("CARGO_PKG_VERSION")),
        });
        let meta_path = dir.join("import.json");
        let mut text =
            serde_json::to_string_pretty(&meta).map_err(|e| ImportError::Write(e.to_string()))?;
        text.push('\n');
        std::fs::write(&meta_path, text).map_err(|e| ImportError::io(&meta_path, e))?;
        Ok(dir)
    }
}

pub(crate) fn file_for(e: &Element) -> &'static str {
    match e.ty.as_str() {
        "Source" => "source.json",
        "Race" | "Species" | "Sub Race" | "Racial Trait" | "Species Trait Option" => "species.json",
        "Class" | "Archetype" | "Archetype Feature" => "classes.json",
        "Class Feature" | "Option" if e.data.contains_key("feature_types") => "options.json",
        "Class Feature" => "classes.json",
        "Background" | "Background Feature" => "backgrounds.json",
        "Feat" | "Feat Feature" => "feats.json",
        "Spell" => "spells.json",
        "Weapon" | "Armor" | "Item" | "Magic Item" | "Mastery Property" => "items.json",
        _ => "misc.json",
    }
}

const FILE_ORDER: &[&str] = &[
    "source.json",
    "species.json",
    "classes.json",
    "backgrounds.json",
    "feats.json",
    "options.json",
    "spells.json",
    "items.json",
    "misc.json",
];

fn type_rank(ty: &str) -> u8 {
    match ty {
        "Source" => 0,
        "Race" | "Species" => 1,
        "Sub Race" => 2,
        "Class" => 3,
        "Archetype" => 4,
        _ => 5,
    }
}

/// Pack id for a source abbreviation.
#[must_use]
pub fn pack_id(source: &str) -> String {
    let cleaned: String = source
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("5etools-{cleaned}")
}

/// Assemble a pack from the elements of one source.
#[must_use]
pub fn build_pack(
    source: &str,
    display_name: &str,
    version: &str,
    mut elements: Vec<Element>,
) -> ImportedPack {
    elements.sort_by(|a, b| (type_rank(&a.ty), &a.id).cmp(&(type_rank(&b.ty), &b.id)));
    let rulesets: Vec<Ruleset> = {
        let mut set: Vec<Ruleset> = elements.iter().filter_map(|e| e.ruleset).collect();
        set.sort();
        set.dedup();
        set
    };
    let pack_ruleset = (rulesets.len() == 1).then(|| rulesets[0]);

    let mut by_file: BTreeMap<&'static str, Vec<Element>> = BTreeMap::new();
    for mut e in elements {
        if pack_ruleset.is_some() {
            e.ruleset = None;
        }
        by_file.entry(file_for(&e)).or_default().push(e);
    }
    let mut files = Vec::new();
    for name in FILE_ORDER {
        let Some(elements) = by_file.remove(name) else {
            continue;
        };
        let mut file = ContentFile::new(elements);
        file.ruleset = pack_ruleset;
        let mut extra = Extra::default();
        extra.attributes.insert("origin".into(), ORIGIN.into());
        extra
            .attributes
            .insert("redistributable".into(), "false".into());
        extra.attributes.insert("source".into(), source.to_string());
        file.info = Some(Info {
            name: Some(format!("{display_name} ({name})")),
            description: Some(
                "Converted at runtime from the user's local 5etools data. Not redistributable."
                    .into(),
            ),
            extra,
            ..Info::default()
        });
        files.push(PackContent {
            path: (*name).to_string(),
            file,
        });
    }
    let manifest = PackManifest {
        format: PackFormat,
        id: pack_id(source),
        name: format!("5etools: {display_name}"),
        version: version.to_string(),
        origin: PackOrigin::Custom,
        rulesets: if rulesets.is_empty() { vec![Ruleset::Dnd2014] } else { rulesets.clone() },
        license: Some("Proprietary: user-supplied 5etools data, not redistributable".into()),
        attribution: None,
        description: Some(format!(
            "origin={ORIGIN}; redistributable=false. Converted at runtime from the user's own 5etools data folder (source {source})."
        )),
        files: files
            .iter()
            .map(|f| PackFile { path: f.path.clone(), kind: PackFileKind::Content })
            .collect(),
    };
    ImportedPack {
        source: source.to_string(),
        manifest,
        files,
        origin: ORIGIN,
        redistributable: false,
        ruleset: pack_ruleset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(id: &str, ty: &str, rs: Ruleset) -> Element {
        let mut e = Element::new(id, id, ty, "Book");
        e.ruleset = Some(rs);
        e
    }

    #[test]
    fn groups_elements_into_files_and_sets_metadata() {
        let mut opt = el("ID_O", "Class Feature", Ruleset::Dnd2014);
        opt.data
            .insert("feature_types".into(), serde_json::json!(["EI"]));
        let pack = build_pack(
            "PHB",
            "Player's Handbook",
            "1.2.3",
            vec![
                el("ID_B_SPELL", "Spell", Ruleset::Dnd2014),
                el("ID_A_CLASS", "Class", Ruleset::Dnd2014),
                el("ID_SRC", "Source", Ruleset::Dnd2014),
                opt,
                el("ID_A_FEATURE", "Class Feature", Ruleset::Dnd2014),
                el("ID_A_SPELL", "Spell", Ruleset::Dnd2014),
                el("ID_MISC", "Condition", Ruleset::Dnd2014),
            ],
        );
        assert_eq!(pack.manifest.id, "5etools-phb");
        assert_eq!(pack.manifest.version, "1.2.3");
        assert_eq!(pack.manifest.origin, PackOrigin::Custom);
        assert_eq!(pack.manifest.rulesets, vec![Ruleset::Dnd2014]);
        assert_eq!(pack.ruleset, Some(Ruleset::Dnd2014));
        assert_eq!(pack.origin, "5etools-import");
        assert!(!pack.redistributable);
        let names: Vec<_> = pack.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "source.json",
                "classes.json",
                "options.json",
                "spells.json",
                "misc.json"
            ]
        );
        assert_eq!(pack.manifest.files.len(), 5);
        let classes = &pack.files[1].file;
        assert_eq!(classes.elements[0].id, "ID_A_CLASS");
        assert_eq!(classes.ruleset, Some(Ruleset::Dnd2014));
        assert!(
            classes.elements[0].ruleset.is_none(),
            "uniform packs carry the ruleset on the file"
        );
        let spells = &pack.files[3].file;
        assert_eq!(spells.elements[0].id, "ID_A_SPELL");
        let info = classes.info.as_ref().unwrap();
        assert_eq!(info.extra.attributes["origin"], "5etools-import");
        assert_eq!(info.extra.attributes["redistributable"], "false");
        assert!(pack
            .manifest
            .description
            .as_ref()
            .unwrap()
            .contains("redistributable=false"));
        assert_eq!(pack.element_count(), 7);
        assert_eq!(pack.elements().count(), 7);
    }

    #[test]
    fn mixed_rulesets_keep_element_rulesets() {
        let pack = build_pack(
            "MIX",
            "Mixed",
            "1.0.0",
            vec![
                el("ID_A", "Spell", Ruleset::Dnd2014),
                el("ID_B", "Spell", Ruleset::Dnd2024),
            ],
        );
        assert_eq!(pack.ruleset, None);
        assert_eq!(
            pack.manifest.rulesets,
            vec![Ruleset::Dnd2014, Ruleset::Dnd2024]
        );
        assert_eq!(pack.files[0].file.ruleset, None);
        assert_eq!(
            pack.files[0].file.elements[1].ruleset,
            Some(Ruleset::Dnd2024)
        );
        assert_eq!(pack_id("UA2020 Subclasses!"), "5etools-ua2020-subclasses-");
    }

    #[test]
    fn writes_loadable_directories() {
        let pack = build_pack(
            "PHB",
            "Player's Handbook",
            "1.0.0",
            vec![el("ID_A", "Spell", Ruleset::Dnd2014)],
        );
        let dir = tempfile::tempdir().unwrap();
        let written = pack.write_to(dir.path()).unwrap();
        assert!(written.ends_with("5etools-phb"));
        let manifest = PackManifest::from_json_str(
            &std::fs::read_to_string(written.join("pack.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest, pack.manifest);
        let content = ContentFile::from_json_str(
            &std::fs::read_to_string(written.join("spells.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(content, pack.files[0].file);
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(written.join("import.json")).unwrap())
                .unwrap();
        assert_eq!(meta["origin"], "5etools-import");
        assert_eq!(meta["redistributable"], false);
        assert_eq!(meta["ruleset"], "dnd2014");
        let store = borealis_content::ContentStoreBuilder::new()
            .add_pack_dir(&written)
            .unwrap()
            .build();
        assert_eq!(store.len(), 1);
    }
}
