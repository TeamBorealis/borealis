//! Spec 4.11 and 18.2 for WB-20: every bundled creature (both monster packs, the 2014 and the 2024 stat block
//! shapes) opens in the creature form, and the form holds all of its stat block:
//!
//! * no part of a stat block is kept from the original (only `renamed_from`, which no form edits, is kept);
//! * written from its field values alone (every stored text forgotten, as if each field had been edited), each
//!   creature is the original element byte for byte, so the canonical writers are the stat block model's shape.
//!
//! The counts are printed (`--nocapture`).

use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{
    element_to_form, form_to_element, CatalogBlockCodec, FormContext, NoLookup, CREATURE_KIND,
};
use borealis_content::{ContentFile, Element, MonsterData, PackFileKind, PackManifest};
use std::fs;
use std::path::{Path, PathBuf};

fn monster_packs() -> Vec<PathBuf> {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut dirs: Vec<PathBuf> = ["srd-5.1-monsters", "srd-5.2.1-monsters"]
        .iter()
        .map(|p| content.join(p))
        .collect();
    dirs.sort();
    dirs
}

fn creatures() -> Vec<Element> {
    let mut out = Vec::new();
    for dir in monster_packs() {
        let manifest = PackManifest::from_json_str(
            &fs::read_to_string(dir.join("pack.json")).expect("pack.json"),
        )
        .expect("manifest");
        for file in manifest
            .files
            .iter()
            .filter(|f| f.kind == PackFileKind::Content)
        {
            let text = fs::read_to_string(dir.join(&file.path)).expect("file");
            let content = ContentFile::from_json_str(&text).expect("content");
            out.extend(content.elements.into_iter().filter(|e| e.ty == "Monster"));
        }
    }
    out
}

#[test]
fn every_bundled_creature_is_held_by_the_creature_form() {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(Vec::<String>::new()),
        lookup: &NoLookup,
        blocks: &codec,
    };
    let all = creatures();
    assert_eq!(all.len(), 647, "the bundled creatures");
    let mut no_kept = 0;
    let mut by_edition = std::collections::BTreeMap::new();
    for e in &all {
        let form = element_to_form(e, &cx);
        assert_eq!(form.kind, CREATURE_KIND, "{}", e.id);
        assert!(form.blocks.is_empty(), "{}: no rules", e.id);
        let kept: Vec<&str> = form
            .preserved
            .iter()
            .filter_map(|p| p.kept.as_ref().map(|k| k.location.as_str()))
            .collect();
        assert!(
            kept.iter().all(|k| *k == "renamed_from"),
            "{}: kept {kept:?}",
            e.id
        );
        if kept.is_empty() {
            no_kept += 1;
        }
        let m = MonsterData::from_element(e).expect("a typed stat block");
        *by_edition.entry(m.ruleset.as_str()).or_insert(0) += 1;

        // from the field values alone
        let mut fresh = form.clone();
        fresh.memo.fields.clear();
        let written = form_to_element(&fresh, &cx).unwrap_or_else(|p| panic!("{}: {p:?}", e.id));
        assert_eq!(
            serde_json::to_string(&written).expect("json"),
            serde_json::to_string(e).expect("json"),
            "{} written from its fields",
            e.id
        );
    }
    println!(
        "creatures: {} ({by_edition:?}), {no_kept} with no kept part, {} keep only renamed_from",
        all.len(),
        all.len() - no_kept
    );
    assert_eq!(no_kept, 618);
}
