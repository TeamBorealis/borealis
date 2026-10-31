//! IM14 (GitHub #11): when the user sets an imported book's edition, a character that already
//! holds content of that book keeps it. R4 still decides new picks (classes never cross
//! editions), but a stored choice of a user-set book is replayed whatever its edition, and the
//! choices under it keep offering the book's options. The compatibility report says why. Content
//! the user did not re-tag is unchanged: a stored choice R4 rejects stays rejected.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

const CLASS: &str = "ID_OB_CLASS";
const BOOK: &str = "Old Book";

fn content(ruleset: &str) -> ContentFile {
    serde_json::from_value(json!({
        "format": "borealis-content/1",
        "ruleset": ruleset,
        "elements": [
            {"id": "ID_OB_SOURCE", "name": BOOK, "type": "Source", "source": "Core"},
            {"id": CLASS, "name": "Oldcaller", "type": "Class", "source": BOOK,
             "setters": [{"name": "hd", "value": "d8"}], "rules": [
                {"kind": "select", "type": "Class Feature", "name": "Gift", "supports": "OB Gift", "level": 1},
                {"kind": "select", "type": "Class Feature", "name": "Second Gift", "supports": "OB Gift", "level": 1}]},
            {"id": "ID_OB_GIFT_A", "name": "Gift A", "type": "Class Feature", "source": BOOK, "supports": ["OB Gift"]},
            {"id": "ID_OB_GIFT_B", "name": "Gift B", "type": "Class Feature", "source": BOOK, "supports": ["OB Gift"]}
        ]}))
    .expect("synthetic content")
}

/// Another 2014 book whose feature also carries the `OB Gift` tag.
fn other_book() -> ContentFile {
    serde_json::from_value(json!({
        "format": "borealis-content/1",
        "ruleset": "dnd2014",
        "elements": [
            {"id": "ID_XB_SOURCE", "name": OTHER, "type": "Source", "source": "Core"},
            {"id": "ID_XB_GIFT_C", "name": "Gift C", "type": "Class Feature", "source": OTHER, "supports": ["OB Gift"]}
        ]}))
    .expect("synthetic content")
}

const OTHER: &str = "Other Book";

fn engine(written: &str, user: Option<Ruleset>) -> Engine {
    let users: Vec<(&str, Ruleset)> = user.map(|r| (BOOK, r)).into_iter().collect();
    engine_with(written, &users)
}

/// Like [`engine`], with the user's edition for any of the books.
fn engine_with(written: &str, users: &[(&str, Ruleset)]) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let editions: BTreeMap<String, Ruleset> = users
        .iter()
        .map(|(b, r)| (borealis_content::source_key(b), *r))
        .collect();
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .add_file("old/book.json", PackOrigin::Custom, content(written))
        .add_file("other/book.json", PackOrigin::Custom, other_book())
        .with_source_editions(editions)
        .build();
    Engine::new(&store)
}

fn slot<'a>(snap: &'a CharacterSnapshot, name: &str) -> &'a SlotSnapshot {
    snap.slots
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
}

/// A character of `ruleset` that took the class and Gift A while the book was `written`.
fn hero(ruleset: Ruleset, written: &str) -> CharacterState {
    let e = engine(written, None);
    let s = commands::new_character(&e, ruleset);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let s = commands::select(&e, &s, &key, Choice::Element(CLASS.into()))
        .expect("class")
        .state;
    let key = slot(&e.derive(&s), "Gift").key.clone();
    commands::select(&e, &s, &key, Choice::Element("ID_OB_GIFT_A".into()))
        .expect("gift")
        .state
}

fn chosen(snap: &CharacterSnapshot, name: &str) -> Option<String> {
    slot(snap, name).chosen.as_ref().map(|c| c.id.clone())
}

fn class_names(e: &Engine, ruleset: Ruleset) -> Vec<String> {
    let snap = e.derive(&commands::new_character(e, ruleset));
    slot(&snap, "Class")
        .options
        .iter()
        .map(|o| o.name.clone())
        .collect()
}

#[test]
fn a_2014_character_keeps_its_class_when_the_user_sets_the_book_to_2024() {
    let s = hero(Ruleset::Dnd2014, "dnd2014");
    let e = engine("dnd2014", Some(Ruleset::Dnd2024));
    let snap = e.derive(&s);
    assert_eq!(chosen(&snap, "Class").as_deref(), Some(CLASS));
    assert!(slot(&snap, "Class").rejected.is_none());
    assert_eq!(chosen(&snap, "Gift").as_deref(), Some("ID_OB_GIFT_A"));
    // a choice under the kept class still offers the book's features (as 2024 content)
    // (the other book's Gift C is 2014 content and native to this 2014 character)
    let mut second: Vec<_> = slot(&snap, "Second Gift")
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect();
    second.sort();
    assert_eq!(second, ["ID_OB_GIFT_B", "ID_XB_GIFT_C"]);
    // the report says why
    let entry = snap
        .compatibility
        .entries
        .iter()
        .find(|c| c.element.id == CLASS)
        .expect("kept class in the report");
    assert_eq!(entry.from, Ruleset::Dnd2024);
    assert!(
        entry.notes.iter().any(|n| n.starts_with("Kept:")),
        "{:?}",
        entry.notes
    );
    // new characters follow the book's edition (R4)
    assert!(!class_names(&e, Ruleset::Dnd2014).contains(&"Oldcaller".to_string()));
    assert!(class_names(&e, Ruleset::Dnd2024).contains(&"Oldcaller".to_string()));
}

#[test]
fn a_2024_character_keeps_its_class_when_the_user_sets_the_book_back_to_2014() {
    let s = hero(Ruleset::Dnd2024, "dnd2024");
    let e = engine("dnd2024", Some(Ruleset::Dnd2014));
    let snap = e.derive(&s);
    assert_eq!(chosen(&snap, "Class").as_deref(), Some(CLASS));
    assert_eq!(chosen(&snap, "Gift").as_deref(), Some("ID_OB_GIFT_A"));
    assert!(!class_names(&e, Ruleset::Dnd2024).contains(&"Oldcaller".to_string()));
}

#[test]
fn content_the_user_did_not_retag_still_follows_r4() {
    // the book's files say 2024 (no user choice): a 2014 character's stored class is rejected
    let s = hero(Ruleset::Dnd2014, "dnd2014");
    let e = engine("dnd2024", None);
    let snap = e.derive(&s);
    assert_eq!(chosen(&snap, "Class"), None);
    assert!(slot(&snap, "Class").rejected.is_some());
}

/// Review H1: under a kept class, only options of the kept class's own book come along. Another
/// book the user also set to 2024 stays out of a 2014 character (R4), as it does when only that
/// other book is set.
#[test]
fn a_kept_class_brings_only_its_own_books_options() {
    let s = hero(Ruleset::Dnd2014, "dnd2014");
    let gifts = |e: &Engine| -> Vec<String> {
        let snap = e.derive(&s);
        let mut ids: Vec<String> = slot(&snap, "Second Gift")
            .options
            .iter()
            .map(|o| o.id.clone())
            .collect();
        ids.sort();
        ids
    };
    // written 2014, nothing set: both books are native
    assert_eq!(
        gifts(&engine_with("dnd2014", &[])),
        ["ID_OB_GIFT_B", "ID_XB_GIFT_C"]
    );
    // only the other book set to 2024: its feature is 2024 and never reaches a 2014 class
    assert_eq!(
        gifts(&engine_with("dnd2014", &[(OTHER, Ruleset::Dnd2024)])),
        ["ID_OB_GIFT_B"]
    );
    // both set to 2024: the kept class keeps its own book's feature, not the other book's
    let both = engine_with(
        "dnd2014",
        &[(BOOK, Ruleset::Dnd2024), (OTHER, Ruleset::Dnd2024)],
    );
    assert_eq!(gifts(&both), ["ID_OB_GIFT_B"]);
}
