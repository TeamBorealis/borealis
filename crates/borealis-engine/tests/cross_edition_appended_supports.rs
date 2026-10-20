//! IM11: supports that content appends onto a 2014 element the 2024 universe supersedes
//! (`maps_to` kind `equivalent`) also apply to its native 2024 equivalent, so a select that
//! matches on them offers the 2024 version. The superseded element stays hidden from new picks,
//! a stored pick of it still replays, and the 2014 universe is unchanged. Only the appended tags
//! move across, never the 2014 element's own supports. Synthetic homebrew class over the bundled
//! core + SRD 5.1 + SRD 5.2.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice, Selection};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const CLASS: &str = "ID_T_ORDER_CLASS";
const MM_14: &str = "ID_PHB_SPELL_MAGIC_MISSILE";
const MM_24: &str = "ID_2024_SPELL_MAGIC_MISSILE";
const OLD_BOLT: &str = "ID_T_OLD_BOLT";
const NEW_BOLT: &str = "ID_T_NEW_BOLT";
const ORDER: &str = "Order Spell";
const CIRCLE: &str = "Circle Spell";

fn spell(id: &str, ruleset: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut v = json!({"id": id, "name": "Test Bolt", "type": "Spell", "source": "Test",
        "ruleset": ruleset,
        "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"}]});
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    v
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1",
        "elements": [
            {"id": CLASS, "name": "Test Orderling", "type": "Class", "source": "Test",
             "setters": [{"name": "hd", "value": "d8"}], "rules": [
                {"kind": "select", "type": "Spell", "name": ORDER, "supports": "T Order", "level": 1},
                {"kind": "select", "type": "Spell", "name": CIRCLE, "supports": "T Old Circle", "level": 1}]},
            spell(OLD_BOLT, "dnd2014", json!({"supports": ["T Old Circle"]})),
            spell(NEW_BOLT, "dnd2024", json!({"maps_to": {"dnd2014": OLD_BOLT, "kind": "equivalent"}}))
        ],
        "appends": [
            {"id": MM_14, "supports": ["T Order"]},
            {"id": OLD_BOLT, "supports": ["T Order"]}
        ]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .add_file("order.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn orderling(e: &Engine, ruleset: Ruleset) -> CharacterState {
    let s = commands::new_character(e, ruleset);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element(CLASS.into()))
        .expect("homebrew class")
        .state
}

fn slot<'a>(snap: &'a CharacterSnapshot, name: &str) -> &'a SlotSnapshot {
    snap.slots
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
}

fn offered(snap: &CharacterSnapshot, name: &str) -> Vec<String> {
    let mut ids: Vec<String> = slot(snap, name)
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect();
    ids.sort();
    ids
}

fn chosen<'a>(snap: &'a CharacterSnapshot, name: &str) -> Option<&'a str> {
    slot(snap, name).chosen.as_ref().map(|c| c.id.as_str())
}

#[test]
fn a_2024_character_is_offered_the_2024_version_of_an_appended_2014_spell() {
    let e = engine();
    let s = orderling(&e, Ruleset::Dnd2024);
    let snap = e.derive(&s);
    assert_eq!(offered(&snap, ORDER), [MM_24, NEW_BOLT]);
    let key = slot(&snap, ORDER).key.clone();
    let picked = commands::select(&e, &s, &key, Choice::Element(MM_24.into()))
        .expect("pick the 2024 spell")
        .state;
    assert_eq!(chosen(&e.derive(&picked), ORDER), Some(MM_24));
    assert!(
        commands::select(&e, &s, &key, Choice::Element(MM_14.into())).is_err(),
        "the superseded 2014 spell is not a new pick"
    );
}

#[test]
fn the_2014_spell_stays_hidden_with_other_edition_content_allowed() {
    let e = engine();
    let mut s = orderling(&e, Ruleset::Dnd2024);
    s.allow_cross_edition = Some(true);
    assert_eq!(offered(&e.derive(&s), ORDER), [MM_24, NEW_BOLT]);
}

#[test]
fn only_appended_supports_move_to_the_2024_equivalent() {
    let e = engine();
    let s = orderling(&e, Ruleset::Dnd2024);
    assert!(
        offered(&e.derive(&s), CIRCLE).is_empty(),
        "the 2014 spell's own supports stay with it"
    );
}

#[test]
fn the_2014_universe_is_unchanged() {
    let e = engine();
    let s = orderling(&e, Ruleset::Dnd2014);
    let snap = e.derive(&s);
    assert_eq!(offered(&snap, ORDER), [MM_14, OLD_BOLT]);
    assert_eq!(offered(&snap, CIRCLE), [OLD_BOLT]);
}

#[test]
fn a_stored_pick_of_the_2014_id_still_replays_in_2024() {
    let e = engine();
    let mut s = orderling(&e, Ruleset::Dnd2024);
    let key = slot(&e.derive(&s), ORDER).key.clone();
    s.selections.push(Selection {
        slot: key,
        choice: Choice::Element(MM_14.into()),
        replace_level: None,
    });
    let snap = e.derive(&s);
    assert_eq!(chosen(&snap, ORDER), Some(MM_14));
    assert!(
        snap.diagnostics
            .iter()
            .all(|d| !d.code.starts_with("selection.") && !d.code.starts_with("eval.")),
        "the stored pick replays without a diagnostic"
    );
}
