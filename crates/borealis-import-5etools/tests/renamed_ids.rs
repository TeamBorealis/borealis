//! A 5etools race that IM8 turned into a 2024 species keeps its former id as an alias
//! (`renamed_from`), which the content store resolves. Invented data only (source `NEWB`).

mod support;

use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use serde_json::{json, Value};
use std::path::Path;
use support::core_pack;

const OLD_ID: &str = "ID_5ET_NEWB_RACE_MOTHFOLK";
const NEW_ID: &str = "ID_5ET_NEWB_SPECIES_MOTHFOLK";
const SCORES: [i32; 6] = [15, 14, 13, 12, 10, 8];

fn write(dir: &Path, rel: &str, v: &Value) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, v.to_string()).unwrap();
}

/// The race; with `dated`, a books.json that dates its book after the 2024 handbook.
fn data(dir: &Path, dated: bool) {
    write(
        dir,
        "races.json",
        &json!({"race": [{"name": "Mothfolk", "source": "NEWB", "size": ["M"], "speed": 35,
            "entries": ["Soft wings."]}]}),
    );
    if dated {
        write(
            dir,
            "books.json",
            &json!({"book": [{"name": "New Book", "id": "NEWB", "source": "NEWB",
                "published": "2025-05-05"}]}),
        );
    }
}

fn store(tmp: &Path, dated: bool) -> (ContentStore, Vec<borealis_content::Element>) {
    let data_dir = tmp.join(if dated { "dated" } else { "old" });
    data(&data_dir, dated);
    let result = import_folder(&data_dir, &ImportOptions::default()).expect("import");
    let content = core_pack().join("..");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1"] {
        builder = builder.add_pack_dir(&content.join(pack)).expect("pack");
    }
    let out = tmp.join(if dated { "packs-dated" } else { "packs-old" });
    for pack in &result.packs {
        let dir = pack.write_to(&out).expect("write pack");
        builder = builder.add_pack_dir(&dir).expect("imported pack loads");
    }
    let elements = result
        .packs
        .iter()
        .flat_map(|p| p.elements().cloned())
        .collect();
    (builder.build(), elements)
}

/// A 2014 fighter of the old race, saved as JSON (what the app stores).
fn saved_character(e: &Engine) -> String {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let mut s = commands::set_base_scores(e, &s, SCORES)
        .expect("scores")
        .state;
    for (slot, id) in [("Race", OLD_ID), ("Class", "ID_WOTC_PHB_CLASS_FIGHTER")] {
        let key = slot_key(&e.derive(&s), "ID_LEVEL_1", slot, 1).expect("slot");
        s = commands::select(e, &s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{id}: {err}"))
            .state;
    }
    serde_json::to_string(&s).expect("save")
}

#[test]
fn a_character_saved_with_a_former_race_id_keeps_its_race() {
    let tmp = tempfile::tempdir().unwrap();
    // before IM8: a 2014 race under the old id
    let (old_store, old_elements) = store(tmp.path(), false);
    assert!(old_elements.iter().any(|e| e.id == OLD_ID));
    let saved = saved_character(&Engine::new(&old_store));
    assert!(saved.contains(OLD_ID));

    // after IM8: a 2024 species that names the old id as its former one
    let (new_store, new_elements) = store(tmp.path(), true);
    let species = new_elements
        .iter()
        .find(|e| e.id == NEW_ID)
        .expect("the species under its new id");
    assert_eq!(species.renamed_from.as_deref(), Some(OLD_ID));
    assert_eq!(
        new_store.element(OLD_ID).map(|e| e.id.as_str()),
        Some(NEW_ID),
        "the store resolves the former id"
    );

    // the app rewrites the saved id on open (src-tauri renamed_ids; tested end to end in
    // src-tauri/tests/fivetools_renamed_ids.rs); the engine itself looks ids up exactly
    let e = Engine::new(&new_store);
    let state: CharacterState = serde_json::from_str(&saved).expect("load");
    assert!(e.derive(&state).invalid_selections.len() == 1);
}
