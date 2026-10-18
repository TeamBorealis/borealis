//! Install-report entries for an imported pack that replaces bundled elements.
//!
//! Imported content loads after the bundled packs and, as in Aurora, the later definition of an
//! id wins. That is faithful but silent, so the report names every replacement: one
//! `element_replaced` warning per source file and bundled pack, with the file, the first id and
//! its type, and every replaced id in the message. Replacing the bundled `core` pack (levels and
//! the internal grants the builder runs on) is called out as such, and a replacement level 1 that
//! offers no Race or Class select says that new characters will have none to pick.

use crate::content_dto::MigrationIssue;
use crate::state::BUNDLED_PACKS;
use borealis_content::{
    ContentFile, ContentStore, Element, IssueCode, PackFileKind, PackManifest, Rule,
};
use std::collections::BTreeMap;
use std::path::Path;

/// The level whose selects create a character.
const FIRST_LEVEL: &str = "ID_LEVEL_1";
/// Select types a new character needs on level 1 (`Species` stands in for `Race` in 2024).
const CREATION_SELECTS: [&[&str]; 2] = [&["Class"], &["Race", "Species"]];

/// The bundled pack that defines `id` in `store`, if any (`file` is `<pack id>/<file>`).
fn bundled_pack<'s>(store: &'s ContentStore, id: &str) -> Option<&'s str> {
    let stored = store.get(id)?;
    let pack = stored.file.split('/').next()?;
    BUNDLED_PACKS.iter().copied().find(|b| *b == pack)
}

/// True when `level` lacks a select of one of the creation types.
fn lacks_creation_selects(level: &Element) -> bool {
    CREATION_SELECTS.iter().any(|tys| {
        !level.rules.iter().any(|r| match r {
            Rule::Select(s) => tys.iter().any(|t| s.ty.eq_ignore_ascii_case(t)),
            _ => false,
        })
    })
}

fn message(pack: &str, ids: &[&Element]) -> String {
    let list = ids.iter().map(|e| e.id.as_str()).collect::<Vec<_>>();
    let what = if pack == crate::state::CORE_PACK {
        "the bundled core rules (levels and internal rules the character builder runs on)".into()
    } else {
        format!("the bundled {pack} pack")
    };
    let mut text = format!(
        "This file replaces {} element(s) of {what}: {}.",
        list.len(),
        list.join(", ")
    );
    if ids
        .iter()
        .any(|e| e.id == FIRST_LEVEL && lacks_creation_selects(e))
    {
        text.push_str(
            " Its level 1 has no Race or Class select, so new characters get no race or class \
             to pick while this pack is enabled.",
        );
    }
    text
}

/// The replacement warnings of the pack written to `out_dir`. `sources` maps an output file to
/// its source file (both relative); a file missing from it is named by its output path.
#[must_use]
pub(crate) fn bundled_override_issues(
    store: &ContentStore,
    out_dir: &Path,
    sources: &BTreeMap<String, String>,
) -> Vec<MigrationIssue> {
    let Some(manifest) = std::fs::read_to_string(out_dir.join("pack.json"))
        .ok()
        .and_then(|t| PackManifest::from_json_str(&t).ok())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in manifest
        .files
        .iter()
        .filter(|f| f.kind == PackFileKind::Content)
    {
        let Some(content) = std::fs::read_to_string(out_dir.join(&entry.path))
            .ok()
            .and_then(|t| ContentFile::from_json_str(t.trim_start_matches('\u{feff}')).ok())
        else {
            continue;
        };
        let mut by_pack: BTreeMap<&str, Vec<&Element>> = BTreeMap::new();
        for e in &content.elements {
            if let Some(pack) = bundled_pack(store, &e.id) {
                by_pack.entry(pack).or_default().push(e);
            }
        }
        let file = sources
            .get(&entry.path)
            .cloned()
            .unwrap_or_else(|| entry.path.clone());
        out.extend(by_pack.into_iter().map(|(pack, ids)| MigrationIssue {
            severity: "warning",
            code: IssueCode::ElementReplaced,
            message: message(pack, &ids),
            element: Some(ids[0].id.clone()),
            element_type: Some(ids[0].ty.clone()),
            file: Some(file.clone()),
            line: None,
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentStoreBuilder, PackOrigin, SelectRule};

    fn level(selects: &[&str]) -> Element {
        let mut e = Element::new(FIRST_LEVEL, "1", "Level", "Internal");
        e.rules = selects
            .iter()
            .map(|t| {
                Rule::Select(SelectRule {
                    ty: (*t).into(),
                    name: (*t).into(),
                    ..SelectRule::default()
                })
            })
            .collect();
        e
    }

    fn write_pack(dir: &Path, files: &[(&str, Vec<Element>)]) {
        let manifest = serde_json::json!({
            "format": "borealis-pack/1", "id": "imported", "name": "Imported", "version": "1.0.0",
            "origin": "custom", "rulesets": ["dnd2014"],
            "files": files.iter().map(|(p, _)| serde_json::json!({"path": p, "kind": "content"}))
                .collect::<Vec<_>>(),
        });
        std::fs::write(dir.join("pack.json"), manifest.to_string()).unwrap();
        for (path, elements) in files {
            let text = ContentFile::new(elements.clone()).to_json_pretty().unwrap();
            std::fs::write(dir.join(path), text).unwrap();
        }
    }

    fn bundled_store() -> ContentStore {
        let core = ContentFile::new(vec![
            level(&["Race", "Class"]),
            Element::new("ID_INTERNAL_GRANTS_X", "X", "Grants", "Internal"),
        ]);
        let srd = ContentFile::new(vec![Element::new("ID_SRD_FEAT", "F", "Feat", "SRD")]);
        ContentStoreBuilder::new()
            .add_file("core/internal.json", PackOrigin::Core, core)
            .add_file("srd-5.1/feats.json", PackOrigin::Core, srd)
            .build()
    }

    #[test]
    fn each_file_names_every_bundled_id_it_replaces_per_pack() {
        let tmp = tempfile::tempdir().unwrap();
        write_pack(
            tmp.path(),
            &[
                (
                    "a.json",
                    vec![
                        level(&["Race", "Class"]),
                        Element::new("ID_INTERNAL_GRANTS_X", "X", "Grants", "Internal"),
                        Element::new("ID_SRD_FEAT", "F", "Feat", "Hb"),
                        Element::new("ID_HB_NEW", "N", "Feat", "Hb"),
                    ],
                ),
                (
                    "b.json",
                    vec![Element::new("ID_HB_OTHER", "O", "Feat", "Hb")],
                ),
            ],
        );
        let sources = BTreeMap::from([("a.json".to_string(), "content/a.xml".to_string())]);
        let issues = bundled_override_issues(&bundled_store(), tmp.path(), &sources);
        assert_eq!(issues.len(), 2, "{issues:?}");
        let core = &issues[0];
        assert_eq!(core.code, IssueCode::ElementReplaced);
        assert_eq!(core.file.as_deref(), Some("content/a.xml"));
        assert_eq!(core.element.as_deref(), Some(FIRST_LEVEL));
        assert_eq!(core.element_type.as_deref(), Some("Level"));
        assert!(core
            .message
            .contains("2 element(s) of the bundled core rules"));
        assert!(core.message.contains("ID_LEVEL_1, ID_INTERNAL_GRANTS_X."));
        assert!(!core.message.contains("no Race or Class"));
        assert!(issues[1]
            .message
            .contains("1 element(s) of the bundled srd-5.1 pack: ID_SRD_FEAT."));
    }

    #[test]
    fn a_level_one_without_creation_selects_is_called_out() {
        let tmp = tempfile::tempdir().unwrap();
        write_pack(tmp.path(), &[("lv.json", vec![level(&["Class"])])]);
        let issues = bundled_override_issues(&bundled_store(), tmp.path(), &BTreeMap::new());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file.as_deref(), Some("lv.json"));
        assert!(issues[0].message.contains("no Race or Class select"));
        assert!(!lacks_creation_selects(&level(&["Class", "Species"])));
    }
}
