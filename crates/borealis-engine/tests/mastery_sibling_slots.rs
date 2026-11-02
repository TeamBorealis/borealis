//! A8: Weapon Mastery slots never master one kind of weapon twice. A weapon a sibling mastery
//! slot holds is neither offered nor accepted, and neither is a `maps_to` equivalent of it (two
//! ids of one element are one element, `engine.md` 6.2 step 4). Synthetic imported copy of the
//! SRD 5.2.1 Battleaxe on the bundled core + SRD 5.2.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use serde_json::json;
use std::path::Path;

const BATTLEAXE: &str = "ID_2024_WEAPON_BATTLEAXE";
const COPY: &str = "ID_T_IMPORTED_BATTLEAXE";
const GREATAXE: &str = "ID_2024_WEAPON_GREATAXE";

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let bundled = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("srd-5.2.1"))
        .expect("pack")
        .build();
    let mut copy =
        serde_json::to_value(&bundled.get(BATTLEAXE).expect("battleaxe").element).unwrap();
    copy["id"] = json!(COPY);
    copy["source"] = json!("Imported");
    copy["maps_to"] = json!({"dnd2024": BATTLEAXE, "kind": "equivalent"});
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2024", "elements": [copy]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("packs")
        .add_file("copy.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn barbarian(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2024);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(
        e,
        &s,
        &key,
        Choice::Element("ID_2024_CLASS_BARBARIAN".into()),
    )
    .expect("barbarian")
    .state
}

fn mastery_slots(e: &Engine, s: &CharacterState) -> Vec<SlotSnapshot> {
    e.derive(s)
        .slots
        .into_iter()
        .filter(|x| x.ty == "Weapon" && x.name.contains("Mastery"))
        .collect()
}

fn offers(slot: &SlotSnapshot, id: &str) -> bool {
    slot.options.iter().any(|o| o.id == id)
}

#[test]
fn an_equivalent_of_a_mastered_weapon_is_not_offered_or_accepted_in_a_sibling_slot() {
    let e = engine();
    let s = barbarian(&e);
    let slots = mastery_slots(&e, &s);
    assert_eq!(slots.len(), 2, "a level 1 Barbarian masters two weapons");
    assert!(offers(&slots[1], COPY) && offers(&slots[1], BATTLEAXE));
    let s = commands::select(&e, &s, &slots[0].key, Choice::Element(COPY.into()))
        .expect("the imported Battleaxe in the first slot")
        .state;
    let slots = mastery_slots(&e, &s);
    assert!(
        offers(&slots[0], COPY),
        "the slot's own choice stays listed"
    );
    assert!(
        !offers(&slots[1], COPY) && !offers(&slots[1], BATTLEAXE),
        "the second slot offers the mastered weapon under one of its ids"
    );
    assert!(offers(&slots[1], GREATAXE));
    for id in [COPY, BATTLEAXE] {
        assert!(
            commands::select(&e, &s, &slots[1].key, Choice::Element(id.into())).is_err(),
            "the second slot accepts {id}"
        );
    }
}
