//! A user library shaped like a full 5etools import (B-235): one pack per book, written by the 5etools
//! importer's own pack writer (about ten files per pack, pretty JSON), filled with copies of the bundled
//! SRD elements under new ids and book names.

use borealis_content::{ContentFile, Element, PackManifest, PackOrigin, Ruleset};
use borealis_import_5etools::pack::build_pack;
use std::path::{Path, PathBuf};

/// The bundled content folder of the source tree.
pub fn content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

const TEMPLATE_PACKS: [&str; 4] = [
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];

/// Every content element of a bundled pack, with its effective ruleset set on the element.
fn template_elements(pack: &str) -> Vec<Element> {
    let dir = content_root().join(pack);
    let manifest =
        PackManifest::from_json_str(&std::fs::read_to_string(dir.join("pack.json")).unwrap())
            .unwrap();
    let pack_ruleset = (manifest.rulesets.len() == 1).then(|| manifest.rulesets[0]);
    let mut out = Vec::new();
    for entry in &manifest.files {
        let text = std::fs::read_to_string(dir.join(&entry.path)).unwrap();
        let Ok(file) = ContentFile::from_json_str(&text) else {
            continue; // index files
        };
        for mut el in file.elements {
            el.ruleset = el.ruleset.or(file.ruleset).or(pack_ruleset);
            if el.ruleset == Some(Ruleset::Any) || el.ruleset.is_none() {
                el.ruleset = Some(Ruleset::Dnd2014);
            }
            out.push(el);
        }
    }
    out
}

/// Write `packs` book packs into `library`; each keeps one in `keep_every` elements of a bundled
/// SRD pack (cycling through them). Returns the number of elements written.
pub fn write_library(library: &Path, packs: usize, keep_every: usize) -> usize {
    std::fs::create_dir_all(library).unwrap();
    let templates: Vec<Vec<Element>> = TEMPLATE_PACKS
        .iter()
        .map(|p| template_elements(p))
        .collect();
    let mut written = 0;
    for i in 0..packs {
        let template = &templates[i % templates.len()];
        let book = format!("Synthetic Book {i:03}");
        let abbr = format!("SYN{i:03}");
        let mut elements: Vec<Element> = template
            .iter()
            .enumerate()
            .filter(|(n, _)| (n + i) % keep_every.max(1) == 0)
            .filter(|(_, e)| e.ty != "Source")
            .map(|(_, e)| {
                let mut e = e.clone();
                e.id = format!("{}_{abbr}", e.id);
                e.source.clone_from(&book);
                e
            })
            .collect();
        let mut source = Element::new(format!("ID_SOURCE_{abbr}"), &book, "Source", "Core");
        source.ruleset = elements.first().and_then(|e| e.ruleset);
        elements.push(source);
        written += elements.len();
        let pack = build_pack(&abbr, &book, "1.0.0", elements);
        assert_eq!(pack.manifest.origin, PackOrigin::Custom);
        pack.write_to(library).unwrap();
    }
    written
}
