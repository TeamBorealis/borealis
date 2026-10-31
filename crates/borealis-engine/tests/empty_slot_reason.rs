//! U14 (GitHub #10): a slot that offers nothing says why. R4 stands (classes never cross editions);
//! the engine counts the options that only the edition hides (`rejected`: no character of this
//! edition can use them; `not_allowed`: the character's "other edition" option would show them) and
//! the options that only the source restriction hides. A list that has options, or whose emptiness
//! has another cause, carries no reason. Bundled core + SRD packs plus synthetic homebrew.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::{
    CharacterSnapshot, EmptyReason, OtherEditionOptions, SlotSnapshot,
};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const SRD14: &str = "SRD 5.1";
const SRD24: &str = "SRD 5.2.1";
const GIFTED: &str = "ID_T_GIFTED_CLASS";

fn homebrew() -> ContentFile {
    serde_json::from_value(json!({
        "format": "borealis-content/1",
        "ruleset": "dnd2014",
        "elements": [
            {"id": GIFTED, "name": "Test Gifted", "type": "Class", "source": "Test Book",
             "setters": [{"name": "hd", "value": "d8"}], "rules": [
                {"kind": "select", "type": "Feat", "name": "Gift", "supports": "T Gift", "level": 1},
                {"kind": "select", "type": "Archetype", "name": "Gifted Path", "supports": "T Gifted Path", "level": 1},
                {"kind": "select", "type": "Class Feature", "name": "Odd Knack", "supports": "T Nothing", "level": 1}]},
            {"id": "ID_T_GIFT_2024", "name": "Gift of Tomorrow", "type": "Feat", "source": "Test Book 2024",
             "ruleset": "dnd2024", "supports": ["T Gift"]},
            {"id": "ID_T_PATH_2024", "name": "Path of Tomorrow", "type": "Archetype", "source": "Test Book 2024",
             "ruleset": "dnd2024", "supports": ["T Gifted Path"]}
        ]}))
    .expect("synthetic content")
}

fn engine(packs: &[&str]) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for p in packs {
        b = b.add_pack_dir(&root.join(p)).expect("bundled pack");
    }
    Engine::new(
        &b.add_file("homebrew.json", PackOrigin::Custom, homebrew())
            .build(),
    )
}

fn hero(e: &Engine, ruleset: Ruleset, restricted: &[&str]) -> CharacterState {
    let mut s = commands::new_character(e, ruleset);
    s.restricted_sources = restricted.iter().map(|r| (*r).to_string()).collect();
    s
}

fn slot<'a>(snap: &'a CharacterSnapshot, name: &str) -> &'a SlotSnapshot {
    snap.slots
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
}

fn class_reason(e: &Engine, s: &CharacterState) -> (usize, Option<EmptyReason>) {
    let snap = e.derive(s);
    let class = slot(&snap, "Class");
    (class.options.len(), class.empty_reason.clone())
}

fn other(edition: Ruleset, rejected: u32, not_allowed: u32) -> Option<OtherEditionOptions> {
    Some(OtherEditionOptions {
        edition,
        rejected,
        not_allowed,
    })
}

#[test]
fn a_2024_character_with_only_2014_classes_learns_why() {
    let e = engine(&["core", "srd-5.1"]);
    let (offered, reason) = class_reason(&e, &hero(&e, Ruleset::Dnd2024, &[]));
    assert_eq!(offered, 0);
    let reason = reason.expect("an empty class list says why");
    // the 12 SRD 5.1 classes and the homebrew class: R4, only a 2014 character can use them
    assert_eq!(reason.other_edition, other(Ruleset::Dnd2014, 13, 0));
    assert_eq!(reason.restricted, 0);
}

#[test]
fn restricted_2024_sources_are_named_beside_the_edition() {
    let e = engine(&["core", "srd-5.1", "srd-5.2.1"]);
    let (offered, reason) = class_reason(&e, &hero(&e, Ruleset::Dnd2024, &[SRD24]));
    assert_eq!(offered, 0);
    let reason = reason.expect("reason");
    assert_eq!(reason.other_edition, other(Ruleset::Dnd2014, 13, 0));
    assert_eq!(
        reason.restricted, 12,
        "the SRD 5.2.1 classes are only restricted"
    );
}

#[test]
fn a_list_with_options_or_a_2014_character_carries_no_reason() {
    let e = engine(&["core", "srd-5.1", "srd-5.2.1"]);
    let (offered, reason) = class_reason(&e, &hero(&e, Ruleset::Dnd2024, &[]));
    assert_eq!((offered, reason), (12, None));
    let (offered, reason) = class_reason(&e, &hero(&e, Ruleset::Dnd2014, &[SRD24]));
    assert_eq!((offered, reason), (13, None));
}

#[test]
fn only_the_restriction_hides_the_classes() {
    let e = engine(&["core", "srd-5.1"]);
    let (offered, reason) = class_reason(&e, &hero(&e, Ruleset::Dnd2014, &[SRD14, "Test Book"]));
    assert_eq!(offered, 0);
    let reason = reason.expect("reason");
    assert_eq!(reason.other_edition, None);
    assert_eq!(reason.restricted, 13);
}

#[test]
fn a_2014_character_without_2014_classes_needs_a_2024_character() {
    let e = engine(&["core", "srd-5.1", "srd-5.2.1"]);
    let (offered, reason) = class_reason(&e, &hero(&e, Ruleset::Dnd2014, &[SRD14, "Test Book"]));
    assert_eq!(offered, 0);
    let reason = reason.expect("reason");
    assert_eq!(reason.other_edition, other(Ruleset::Dnd2024, 12, 0));
    assert_eq!(reason.restricted, 13);
}

#[test]
fn other_single_edition_lists_share_the_reason() {
    let e = engine(&["core", "srd-5.1"]);
    let s = hero(&e, Ruleset::Dnd2014, &[]);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let mut s = commands::select(&e, &s, &key, Choice::Element(GIFTED.into()))
        .expect("homebrew class")
        .state;
    let snap = e.derive(&s);
    // a 2024 subclass never crosses into a 2014 character (R4): another character is needed
    let path = slot(&snap, "Gifted Path");
    assert!(path.options.is_empty());
    assert_eq!(
        path.empty_reason
            .as_ref()
            .and_then(|r| r.other_edition.clone()),
        other(Ruleset::Dnd2024, 1, 0)
    );
    // a 2024 feat may cross, but this strict 2014 character does not allow the other edition
    let gift = slot(&snap, "Gift");
    assert!(gift.options.is_empty());
    assert_eq!(
        gift.empty_reason
            .as_ref()
            .and_then(|r| r.other_edition.clone()),
        other(Ruleset::Dnd2024, 0, 1)
    );
    // nothing matches at all: no reason to give
    assert_eq!(slot(&snap, "Odd Knack").empty_reason, None);
    // with the other edition allowed the feat is offered and the reason goes
    s.allow_cross_edition = Some(true);
    let snap = e.derive(&s);
    assert_eq!(slot(&snap, "Gift").options.len(), 1);
    assert_eq!(slot(&snap, "Gift").empty_reason, None);
}

/// An option its native equivalent replaces (IM11 `maps_to` kind `equivalent`) is not "hidden by
/// the other-edition switch": switching it on would not bring it back. Found by the a-comm-combo
/// gate run (a 2014 race whose language select names a tag only the replaced 2014 languages carry).
#[test]
fn replaced_options_are_not_blamed_on_the_edition_switch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1",
        "elements": [
            {"id": "ID_T_OLD_RACE", "name": "Old Folk", "type": "Race", "source": "Test Book", "ruleset": "dnd2014",
             "rules": [{"kind": "select", "type": "Language", "name": "Old Tongue", "supports": "T Starting", "level": 1}]},
            {"id": "ID_T_OLD_TONGUE", "name": "Old Tongue", "type": "Language", "source": "Test Book",
             "ruleset": "dnd2014", "supports": ["T Starting"]},
            {"id": "ID_T_NEW_TONGUE", "name": "Old Tongue", "type": "Language", "source": "Test Book 2024",
             "ruleset": "dnd2024", "supports": ["Standard"],
             "maps_to": {"dnd2014": "ID_T_OLD_TONGUE", "kind": "equivalent"}}
        ]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .add_file("old.json", PackOrigin::Custom, file)
        .build();
    let e = Engine::new(&store);
    let s = commands::new_character(&e, Ruleset::Dnd2024);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Race", 1).expect("race slot");
    let s = commands::select(&e, &s, &key, Choice::Element("ID_T_OLD_RACE".into()))
        .expect("2014 race")
        .state;
    let snap = e.derive(&s);
    let tongue = slot(&snap, "Old Tongue");
    assert!(tongue.options.is_empty());
    assert_eq!(tongue.empty_reason, None);
}
