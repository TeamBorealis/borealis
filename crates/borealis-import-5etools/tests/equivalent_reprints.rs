//! T11 repro (engine, not fixed here): an imported 5etools spell that is a reprint declares
//! both counterparts, `maps_to {dnd2014: <SRD 5.1 id>, dnd2024: <SRD 5.2.1 id>, kind:
//! equivalent}` (the importer's `Shared::maps_to`, from the SRD index and `reprintedAs`). With
//! the bundled SRD 5.1 and 5.2.1 packs loaded (F-full), the engine's equivalence index keeps one
//! target per element (the 2024 side overwrites the 2014 side) and follows one hop only, so the
//! SRD 5.1 Chill Touch and the 5etools PHB Chill Touch are not seen as one element: a Warlock
//! holding one is still offered and accepts the other in a sibling cantrip slot.
//!
//! Ignored until the engine fix lands (`borealis-engine/src/universe.rs index_equivalents`:
//! equivalence classes over every `maps_to` equivalent edge, both sides, transitively). Run with
//! `cargo test -p borealis-import-5etools --test equivalent_reprints -- --ignored`.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const SRD_51: &str = "ID_PHB_SPELL_CHILL_TOUCH";
const SRD_521: &str = "ID_2024_SPELL_CHILL_TOUCH";
const COPY: &str = "ID_5ET_PHB_SPELL_CHILL_TOUCH";
const WARLOCK: &str = "ID_WOTC_PHB_CLASS_WARLOCK";

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    // the shape the importer writes for the PHB Chill Touch (reprinted in the XPHB)
    let copy = json!({"id": COPY, "name": "Chill Touch", "type": "Spell", "source": "Imported PHB",
        "supports": ["Sorcerer", "Warlock", "Wizard"],
        "setters": [{"name": "level", "value": "0"}, {"name": "school", "value": "Necromancy"}],
        "maps_to": {"dnd2014": SRD_51, "dnd2024": SRD_521, "kind": "equivalent"}});
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [copy]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .add_file("copy.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn warlock(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element(WARLOCK.into()))
        .expect("warlock")
        .state
}

/// Pick `first` in a cantrip slot, then try `second` in every other open cantrip slot.
fn second_pick_accepted(e: &Engine, first: &str, second: &str) -> bool {
    let s = warlock(e);
    let snap = e.derive(&s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.ty == "Spell" && x.options.iter().any(|o| o.id == first))
        .expect("a cantrip slot offering the first pick");
    let s = commands::select(e, &s, &slot.key, Choice::Element(first.into()))
        .expect("first pick")
        .state;
    e.derive(&s)
        .slots
        .iter()
        .filter(|x| x.ty == "Spell" && x.chosen.is_none())
        .any(|x| commands::select(e, &s, &x.key, Choice::Element(second.into())).is_ok())
}

#[test]
#[ignore = "T11: engine equivalence index keeps one maps_to target per element (borealis-engine owner)"]
fn a_reprint_copy_and_its_srd_51_counterpart_are_one_cantrip() {
    let e = engine();
    assert!(
        !second_pick_accepted(&e, SRD_51, COPY),
        "the 5etools copy is accepted next to the SRD 5.1 Chill Touch"
    );
    assert!(
        !second_pick_accepted(&e, COPY, SRD_51),
        "the SRD 5.1 Chill Touch is accepted next to the 5etools copy"
    );
}

#[test]
fn a_copy_with_one_counterpart_is_refused_as_a_second_pick() {
    // control: the same copy declaring only its 2014 side is one element with it (139047b)
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let copy = json!({"id": COPY, "name": "Chill Touch", "type": "Spell", "source": "Imported PHB",
        "supports": ["Sorcerer", "Warlock", "Wizard"],
        "setters": [{"name": "level", "value": "0"}, {"name": "school", "value": "Necromancy"}],
        "maps_to": {"dnd2014": SRD_51, "kind": "equivalent"}});
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [copy]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("copy.json", PackOrigin::Custom, file)
        .build();
    let e = Engine::new(&store);
    assert!(!second_pick_accepted(&e, SRD_51, COPY));
    assert!(!second_pick_accepted(&e, COPY, SRD_51));
}
