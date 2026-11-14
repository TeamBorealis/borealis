//! `EngineSettings::hidden_elements`: listed elements are never offered for new picks, as a superseded element
//! isn't, while a stored choice of one still replays. An app uses it to show one copy of an element printed in
//! several installed sources. Bundled core + SRD packs plus a small book with two copies of one feat.

use std::collections::BTreeSet;
use std::path::Path;

use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::Choice;
use borealis_engine::{Engine, EngineSettings};
use serde_json::json;

const CLASS: &str = "ID_T_WALKER";
const COPY_A: &str = "ID_T_GIFT_A";
const COPY_B: &str = "ID_T_GIFT_B";

fn store() -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
    "format": "borealis-content/1",
    "elements": [
        {"id": CLASS, "name": "Walker", "type": "Class", "source": "Book A", "ruleset": "dnd2014",
         "setters": [{"name": "hd", "value": "d8"}],
         "rules": [{"kind": "select", "type": "Feat", "name": "Walker's Gift", "supports": "T Gift", "level": 1}]},
        {"id": COPY_A, "name": "Walker's Gift", "type": "Feat", "source": "Book A", "ruleset": "dnd2014",
         "supports": ["T Gift"]},
        {"id": COPY_B, "name": "Walker's Gift", "type": "Feat", "source": "Book B", "ruleset": "dnd2014",
         "supports": ["T Gift"]}
    ]}))
    .expect("synthetic content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("book.json", PackOrigin::Custom, file)
        .build()
}

fn hiding(store: &ContentStore, ids: &[&str]) -> Engine {
    Engine::with_settings(
        store,
        EngineSettings {
            hidden_elements: ids
                .iter()
                .map(|s| (*s).to_string())
                .collect::<BTreeSet<_>>(),
            ..EngineSettings::default()
        },
    )
}

/// A 2014 character with the Walker class.
fn walker(e: &Engine) -> borealis_engine::state::CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element(CLASS.into()))
        .expect("class")
        .state
}

fn gift_options(snap: &CharacterSnapshot) -> Vec<String> {
    let mut ids: Vec<String> = snap
        .slots
        .iter()
        .find(|x| x.name == "Walker's Gift")
        .expect("gift slot")
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn hidden_elements_are_not_offered() {
    let store = store();
    let plain = Engine::new(&store);
    assert_eq!(
        gift_options(&plain.derive(&walker(&plain))),
        [COPY_A, COPY_B]
    );
    let e = hiding(&store, &[COPY_B]);
    assert_eq!(gift_options(&e.derive(&walker(&e))), [COPY_A]);
    assert!(
        e.universe(Ruleset::Dnd2014)
            .get(COPY_B)
            .expect("loaded")
            .hidden
    );
    assert!(
        !e.universe(Ruleset::Dnd2014)
            .get(COPY_A)
            .expect("loaded")
            .hidden
    );
}

/// A character that chose a copy before it was hidden keeps it.
#[test]
fn a_stored_choice_of_a_hidden_element_still_replays() {
    let store = store();
    let plain = Engine::new(&store);
    let s = walker(&plain);
    let key = commands::slot_key(&plain.derive(&s), CLASS, "Walker's Gift", 1).expect("gift slot");
    let s = commands::select(&plain, &s, &key, Choice::Element(COPY_B.into()))
        .expect("gift")
        .state;
    let e = hiding(&store, &[COPY_B]);
    let snap = e.derive(&s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.name == "Walker's Gift")
        .expect("gift slot");
    assert_eq!(slot.chosen.as_ref().map(|c| c.id.as_str()), Some(COPY_B));
    assert!(slot.rejected.is_none() && slot.missing.is_none());
}
