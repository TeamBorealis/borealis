//! HC20 overrides in a pack (spec 22): split on save, saved in place, reverted with their parts, change detection,
//! and `creator.json` kept in step with the content.

use super::*;
use crate::pack::{digest, CopiedFrom, CreatorFile};
use borealis_content::{ContentFile, Element, Ruleset};
use std::collections::HashMap;

fn el(id: &str, ty: &str, text: &str) -> Element {
    let mut e = Element::new(id, id, ty, "System Reference Document 5.1");
    e.description = text.into();
    e.ruleset = Some(Ruleset::Dnd2014);
    e
}

fn entry(part_of: Option<&str>, digest: &str) -> CreatorOverride {
    CreatorOverride {
        kind: "other".into(),
        created_at: "2026-11-26T20:00:00Z".into(),
        original: Some(CopiedFrom {
            id: "ID_X".into(),
            pack_id: "srd-5.1".into(),
            srd: true,
        }),
        original_digest: digest.into(),
        part_of: part_of.map(str::to_string),
        non_srd_text: false,
    }
}

#[test]
fn the_main_element_is_always_an_override_and_unchanged_children_are_not() {
    let originals: HashMap<String, Element> = [
        el("ID_SRD_CLASS_FIGHTER", "Class", "a"),
        el("ID_SRD_FEATURE_A", "Class Feature", "a"),
        el("ID_SRD_FEATURE_B", "Class Feature", "b"),
    ]
    .into_iter()
    .map(|e| (e.id.clone(), e))
    .collect();
    let mut changed_b = el("ID_SRD_FEATURE_B", "Class Feature", "b changed");
    changed_b.ruleset = None;
    let mut unchanged_a = el("ID_SRD_FEATURE_A", "Class Feature", "a");
    unchanged_a.ruleset = None; // the form may write no edition; it is still the original
    let written = vec![
        el("ID_SRD_CLASS_FIGHTER", "Class", "a"),
        unchanged_a,
        changed_b,
        el("ID_HB_CLASS_FEATURE_NEW", "Class Feature", "new"),
    ];
    let before: HashSet<&str> = ["ID_SRD_FEATURE_A"].into_iter().collect();
    let split = split_override(written, |id| originals.get(id), &before, "My Pack");
    let ids: Vec<&str> = split.overrides.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["ID_SRD_CLASS_FIGHTER", "ID_SRD_FEATURE_B"]);
    assert_eq!(
        split.reverted,
        ["ID_SRD_FEATURE_A"],
        "an earlier child override back to equal is dropped"
    );
    assert_eq!(split.own.len(), 1);
    assert_eq!(
        split.own[0].source, "My Pack",
        "a new child is the pack's own"
    );
    assert_eq!(
        split.overrides[0].source, "System Reference Document 5.1",
        "an override keeps its book"
    );
}

#[test]
fn overrides_are_saved_in_place_and_removed_with_their_parts() {
    let content = with_overrides(
        &ContentFile::new(vec![el("ID_HB_SOURCE", "Source", "")]),
        &[
            el("ID_SRD_CLASS_FIGHTER", "Class", "1"),
            el("ID_SRD_FEATURE_B", "Class Feature", "1"),
        ],
    );
    let content = with_overrides(&content, &[el("ID_SRD_CLASS_FIGHTER", "Class", "2")]);
    assert_eq!(content.overrides.len(), 2);
    assert_eq!(content.overrides[0].description, "2", "replaced in place");
    assert_eq!(content.elements.len(), 1, "own elements are untouched");
    let creator = CreatorFile::new("HB", "1.5.1")
        .with_override("ID_SRD_CLASS_FIGHTER", entry(None, "d1"))
        .with_override(
            "ID_SRD_FEATURE_B",
            entry(Some("ID_SRD_CLASS_FIGHTER"), "d2"),
        );
    assert_eq!(
        override_parts(&content, &creator, "ID_SRD_CLASS_FIGHTER"),
        ["ID_SRD_CLASS_FIGHTER", "ID_SRD_FEATURE_B"]
    );
    let (next, record, removed) = revert_override(&content, &creator, "ID_SRD_CLASS_FIGHTER");
    assert!(next.overrides.is_empty());
    assert_eq!(removed.len(), 2);
    assert!(record.overrides.is_empty());
    assert!(
        record.retired_ids.is_empty(),
        "a reverted id is not the pack's: never retired"
    );
    assert_eq!(next.elements, content.elements);
}

#[test]
fn reverting_a_child_alone_keeps_the_main_override() {
    let content = with_overrides(
        &ContentFile::new(vec![]),
        &[
            el("ID_SRD_CLASS_FIGHTER", "Class", "1"),
            el("ID_SRD_FEATURE_B", "Class Feature", "1"),
        ],
    );
    let creator = CreatorFile::new("HB", "1.5.1")
        .with_override("ID_SRD_CLASS_FIGHTER", entry(None, "d1"))
        .with_override(
            "ID_SRD_FEATURE_B",
            entry(Some("ID_SRD_CLASS_FIGHTER"), "d2"),
        );
    let (next, record, _) = revert_override(&content, &creator, "ID_SRD_FEATURE_B");
    assert_eq!(next.overrides.len(), 1);
    assert!(record.overrides.contains_key("ID_SRD_CLASS_FIGHTER"));
}

#[test]
fn the_record_keeps_the_first_save_time_and_follows_the_content() {
    let creator = CreatorFile::new("HB", "1.5.1").with_override("ID_SRD_X", entry(None, "d1"));
    let mut later = entry(None, "d2");
    later.created_at = "2026-11-27T10:00:00Z".into();
    let creator = creator.with_override("ID_SRD_X", later);
    let o = &creator.overrides["ID_SRD_X"];
    assert_eq!(o.created_at, "2026-11-26T20:00:00Z");
    assert_eq!(o.original_digest, "d2");
    // an override the content no longer has loses its entry; one made outside the creator gets one
    let content = with_overrides(&ContentFile::new(vec![]), &[el("ID_SRD_Y", "Spell", "y")]);
    let reconciled = creator.reconciled(&content);
    assert!(!reconciled.overrides.contains_key("ID_SRD_X"));
    let y = &reconciled.overrides["ID_SRD_Y"];
    assert!(y.original.is_none() && y.original_digest.is_empty());
    assert!(
        !reconciled.elements.contains_key("ID_SRD_Y"),
        "an override is no element of the pack"
    );
}

#[test]
fn a_non_srd_override_makes_the_pack_not_redistributable() {
    let mut e = entry(None, "d");
    assert!(!CreatorFile::new("HB", "1")
        .with_override("ID_A", e.clone())
        .has_non_srd_text());
    e.non_srd_text = true;
    assert!(CreatorFile::new("HB", "1")
        .with_override("ID_A", e)
        .has_non_srd_text());
}

#[test]
fn change_detection_compares_the_recorded_digest() {
    let original = el("ID_SRD_X", "Spell", "x");
    let now = digest(&[&original]);
    assert!(!original_changed(Some(&entry(None, &now)), &now));
    assert!(original_changed(Some(&entry(None, "0000")), &now));
    assert!(
        !original_changed(Some(&entry(None, "")), &now),
        "no recorded digest: unknown, not changed"
    );
    assert!(!original_changed(None, &now));
}

#[test]
fn overrides_and_their_records_go_to_the_overrides_file_not_creator_json() {
    let content = with_overrides(
        &ContentFile::new(vec![el("ID_HB_OWN", "Spell", "own")]),
        &[el("ID_SRD_X", "Spell", "x")],
    );
    let creator =
        CreatorFile::new("HB", "1.5.1").with_override("ID_SRD_X", entry(Some("ID_SRD_Y"), "d"));
    // creator.json and content.json carry nothing Borealis 1.5.0 doesn't know
    let creator_text = creator.to_json_pretty();
    assert!(
        !creator_text.contains("overrides") && !creator_text.contains("ID_SRD_X"),
        "{creator_text}"
    );
    let disk = content_on_disk(&content);
    assert!(disk.overrides.is_empty());
    assert_eq!(disk.elements, content.elements);
    assert!(!disk.to_json_pretty().unwrap().contains("overrides"));
    // the overrides file holds both, and reads back
    let file = overrides_file(&content, &creator).expect("a file");
    let text = file.to_json_pretty().unwrap();
    assert!(
        text.contains("\"originalDigest\": \"d\"") && text.contains("\"partOf\": \"ID_SRD_Y\""),
        "{text}"
    );
    let back = borealis_content::OverridesFile::from_json_str(&text).unwrap();
    assert_eq!(back.overrides, content.overrides);
    let records = override_records(&back);
    assert_eq!(
        CreatorFile::parse(&creator_text)
            .unwrap()
            .with_override_records(&records),
        creator
    );
    // no override, no file
    assert!(overrides_file(&disk, &creator).is_none());
}

#[test]
fn the_main_element_stays_an_override_while_its_original_is_not_loaded() {
    let split = split_override(
        vec![el("ID_BOOK_FEAT", "Feat", "mine")],
        |_| None,
        &HashSet::new(),
        "My Pack",
    );
    assert_eq!(split.overrides.len(), 1);
    assert!(split.own.is_empty());
}
