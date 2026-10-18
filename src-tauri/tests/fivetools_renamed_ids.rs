//! A character saved with a 5etools id that a later import renamed keeps its pick: IM8 turned
//! the races of books dated after the 2024 Player's Handbook into species
//! (`ID_5ET_<SRC>_RACE_X` -> `ID_5ET_<SRC>_SPECIES_X`), the importer lists the former id as
//! `renamed_from`, and the app rewrites it when the character is opened
//! (`borealis_lib::renamed_ids::migrate`). Invented data only (source `NEWB`).

use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const OLD_ID: &str = "ID_5ET_NEWB_RACE_MOTHFOLK";
const NEW_ID: &str = "ID_5ET_NEWB_SPECIES_MOTHFOLK";
const SPEED: i32 = 35;
const SCORES: [i32; 6] = [15, 14, 13, 12, 10, 8];

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn write(dir: &Path, rel: &str, v: &Value) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, v.to_string()).unwrap();
}

/// The race imported without (`dated` false) or with a books.json dating its book after the
/// 2024 handbook.
fn store(tmp: &Path, dated: bool) -> ContentStore {
    let data = tmp.join(if dated { "dated" } else { "old" });
    write(
        &data,
        "races.json",
        &json!({"race": [{"name": "Mothfolk", "source": "NEWB", "size": ["M"], "speed": SPEED,
            "entries": ["Soft wings."]}]}),
    );
    if dated {
        write(
            &data,
            "books.json",
            &json!({"book": [{"name": "New Book", "id": "NEWB", "source": "NEWB",
                "published": "2025-05-05"}]}),
        );
    }
    let result = import_folder(&data, &ImportOptions::default()).expect("import");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1"] {
        builder = builder
            .add_pack_dir(&content_dir().join(pack))
            .expect("bundled pack");
    }
    let out = tmp.join(if dated { "packs-dated" } else { "packs-old" });
    for pack in &result.packs {
        let dir = pack.write_to(&out).expect("write pack");
        builder = builder.add_pack_dir(&dir).expect("imported pack loads");
    }
    builder.build()
}

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
    let old_store = store(tmp.path(), false);
    let old = Engine::new(&old_store);
    let saved = saved_character(&old);
    assert_eq!(
        old.derive(&serde_json::from_str(&saved).unwrap())
            .speed
            .walk,
        SPEED
    );

    let new_store = store(tmp.path(), true);
    let mut state: CharacterState = serde_json::from_str(&saved).expect("load");
    let changed = borealis_lib::renamed_ids::migrate(&mut state, &new_store);
    assert_eq!(changed, 1, "the race id is rewritten once");
    let json = serde_json::to_string(&state).unwrap();
    assert!(json.contains(NEW_ID) && !json.contains(OLD_ID), "{json}");

    // the saved pick now names an element the content has (no missing-content diagnostic)
    let snap = Engine::new(&new_store).derive(&state);
    assert!(
        !snap
            .diagnostics
            .iter()
            .any(|d| d.code == "selection.missing_content"),
        "{:?}",
        snap.diagnostics
    );
    // GAP (engine): the element is a 2024 Species now, and a 2014 Race slot keeps only `Race`
    // elements, even with "allow content from the other edition" on; the species carries
    // `legacy_alias: "Race"` for the engine to accept it (specs/handoffs/det-5e-4.md).
    let species = new_store.element(NEW_ID).expect("species");
    assert_eq!(species.legacy_alias.as_deref(), Some("Race"));
    assert_eq!(species.renamed_from.as_deref(), Some(OLD_ID));
}
