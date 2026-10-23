//! B-44b: a 2014 subclass whose 2024 reprint has a new name (profile rename table) is hidden
//! from a 2024 class when the reprint is loaded, and offered when it is not. Synthetic ids.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

const WIZARD_24: &str = "ID_2024_CLASS_WIZARD";
const WIZARD_CHOICE_24: &str = "ID_2024_CLASS_FEATURE_WIZARD_WIZARD_SUBCLASS";
const WARLOCK_24: &str = "ID_2024_CLASS_WARLOCK";
const WARLOCK_CHOICE_24: &str = "ID_2024_CLASS_FEATURE_WARLOCK_WARLOCK_SUBCLASS";
const ABJ_14: &str = "ID_T_B44B_SCHOOL_ABJ";
const ABJ_24: &str = "ID_T_B44B_ABJURER";
const FEY_14: &str = "ID_T_B44B_ARCHFEY";
const FEY_24: &str = "ID_T_B44B_ARCHFEY_PATRON";

fn el(id: &str, name: &str, tag: &str) -> Value {
    json!({"id": id, "name": name, "type": "Archetype", "source": "Test", "supports": [tag]})
}

fn file(ruleset: &str, elements: Vec<Value>) -> ContentFile {
    let f = json!({"format": "borealis-content/1", "ruleset": ruleset, "elements": elements});
    serde_json::from_value(f).expect("synthetic content")
}

fn engine(with_2024_reprints: bool) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let reprints = if with_2024_reprints {
        vec![
            el(ABJ_24, "Abjurer", "Wizard Subclass"),
            el(FEY_24, "Archfey Patron", "Warlock Subclass"),
        ]
    } else {
        vec![]
    };
    let older = vec![
        el(ABJ_14, "School of Abjuration", "Arcane Tradition"),
        el(FEY_14, "The Archfey", "Otherworldly Patron"),
    ];
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .add_file("b44b.json", PackOrigin::Custom, file("dnd2014", older))
        .add_file(
            "b44b-2024.json",
            PackOrigin::Custom,
            file("dnd2024", reprints),
        )
        .build();
    Engine::new(&store)
}

fn offered_at_3(e: &Engine, class: &str, owner: &str) -> Vec<String> {
    let mut s: CharacterState = commands::new_character(e, Ruleset::Dnd2024);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .expect("class")
        .state;
    for _ in 1..3 {
        s = commands::level_up_main(e, &s, None).expect("level").state;
    }
    let snap = e.derive(&s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.owner.id == owner)
        .unwrap_or_else(|| panic!("no slot for {owner}"));
    slot.options.iter().map(|o| o.id.clone()).collect()
}

#[test]
fn a_renamed_2024_reprint_hides_the_2014_wizard_and_warlock_subclass() {
    let e = engine(true);
    let wizard = offered_at_3(&e, WIZARD_24, WIZARD_CHOICE_24);
    assert!(wizard.contains(&ABJ_24.to_string()), "{wizard:?}");
    assert!(!wizard.contains(&ABJ_14.to_string()), "{wizard:?}");
    let warlock = offered_at_3(&e, WARLOCK_24, WARLOCK_CHOICE_24);
    assert!(warlock.contains(&FEY_24.to_string()), "{warlock:?}");
    assert!(!warlock.contains(&FEY_14.to_string()), "{warlock:?}");
}

#[test]
fn the_2014_subclass_is_offered_when_no_renamed_reprint_is_loaded() {
    let e = engine(false);
    let wizard = offered_at_3(&e, WIZARD_24, WIZARD_CHOICE_24);
    assert!(wizard.contains(&ABJ_14.to_string()), "{wizard:?}");
    let warlock = offered_at_3(&e, WARLOCK_24, WARLOCK_CHOICE_24);
    assert!(warlock.contains(&FEY_14.to_string()), "{warlock:?}");
}
