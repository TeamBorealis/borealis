//! st3-diff F13: an imported copy of an element that declares itself `maps_to` kind
//! `equivalent` of a loaded element is the same rules element under two ids. A select offers
//! it once (the counterpart while that is offerable) and refuses a second pick of it
//! (`engine.md` 6.2 step 4: duplicates are removed per element). Synthetic copy of the SRD 5.1
//! Chill Touch on the bundled core + SRD 5.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const SRD: &str = "ID_PHB_SPELL_CHILL_TOUCH";
const COPY: &str = "ID_T_IMPORTED_CHILL_TOUCH";
const WARLOCK: &str = "ID_WOTC_PHB_CLASS_WARLOCK";

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let copy = json!({"id": COPY, "name": "Chill Touch", "type": "Spell", "source": "Imported PHB",
        "supports": ["Sorcerer", "Warlock", "Wizard"],
        "setters": [{"name": "level", "value": "0"}, {"name": "school", "value": "Necromancy"}],
        "maps_to": {"dnd2014": SRD, "kind": "equivalent"}});
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [copy]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
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

fn cantrip_slots(e: &Engine, s: &CharacterState) -> Vec<SlotSnapshot> {
    e.derive(s)
        .slots
        .into_iter()
        .filter(|x| x.ty == "Spell" && x.options.iter().any(|o| o.id == SRD || o.id == COPY))
        .collect()
}

fn offers(slot: &SlotSnapshot, id: &str) -> bool {
    slot.options.iter().any(|o| o.id == id)
}

#[test]
fn an_equivalent_copy_is_offered_once() {
    let e = engine();
    let s = warlock(&e);
    let slots = cantrip_slots(&e, &s);
    assert!(!slots.is_empty(), "a cantrip slot offers Chill Touch");
    for slot in &slots {
        assert!(offers(slot, SRD) && !offers(slot, COPY), "{}", slot.name);
    }
}

#[test]
fn the_copy_is_offered_when_its_counterpart_is_restricted_and_blocks_a_second_pick() {
    let e = engine();
    let mut s = warlock(&e);
    s.restricted_sources.push("SRD 5.1".into());
    let slots = cantrip_slots(&e, &s);
    let first = slots.first().expect("slot offering the copy");
    assert!(offers(first, COPY) && !offers(first, SRD));
    let mut s = commands::select(&e, &s, &first.key, Choice::Element(COPY.into()))
        .expect("pick the copy")
        .state;
    s.restricted_sources.clear();
    let snap = e.derive(&s);
    let open: Vec<_> = snap
        .slots
        .iter()
        .filter(|x| x.ty == "Spell" && x.chosen.is_none())
        .collect();
    assert!(!open.is_empty(), "a second open cantrip slot");
    for slot in &open {
        assert!(
            !offers(slot, SRD),
            "{} offers the counterpart of a held copy",
            slot.name
        );
        assert!(
            commands::select(&e, &s, &slot.key, Choice::Element(SRD.into())).is_err(),
            "{} accepts the counterpart of a held copy",
            slot.name
        );
    }
}

#[test]
fn improvement_options_are_listed_once_with_both_srd_packs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .build();
    let e = Engine::new(&store);
    let s = commands::new_character(&e, Ruleset::Dnd2014);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let mut s = commands::select(
        &e,
        &s,
        &key,
        Choice::Element("ID_WOTC_PHB_CLASS_FIGHTER".into()),
    )
    .expect("fighter")
    .state;
    while s.level() < 4 {
        s = commands::level_up_main(&e, &s, None)
            .expect("level up")
            .state;
    }
    let snap = e.derive(&s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.name.starts_with("Improvement Option"))
        .expect("improvement slot at 4");
    let mut ids: Vec<&str> = slot.options.iter().map(|o| o.id.as_str()).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "{ids:?}");
}
