//! Several companions (GitHub #13, decision U16(c)) in Borealis's own file format: files written
//! before the feature load with no extra companions and save byte for byte as before, a
//! character with edits and added companions round-trips, and the Aurora import of a companion
//! is unchanged.

mod common;
use borealis_character::{
    import_dnd5e, AddedCompanion, Character, CompanionEdits, CompanionTrait, Companions,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

const EXAMPLE: &str = include_str!("data/example.dnd5e");

fn fixtures(dir: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir);
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixtures")
        .map(|e| {
            let path = e.expect("entry").path();
            let text = std::fs::read_to_string(&path).expect("read");
            (path.display().to_string(), text)
        })
        .collect();
    out.sort();
    out
}

/// Saves written by 1.2.2 (3ca2681): three with no companion name, and one with a named and
/// pictured companion. Loading and saving them now gives the same bytes.
#[test]
fn files_with_zero_or_one_companion_save_byte_for_byte_as_before() {
    let files: Vec<_> = fixtures("prefeature-1.2")
        .into_iter()
        .chain(fixtures("companion-1.2.2"))
        .collect();
    assert_eq!(files.len(), 4);
    for (path, before) in files {
        let c = Character::from_json_str(&before).expect("load");
        assert!(c.companions.is_empty(), "{path}");
        assert_eq!(c.to_json_string().expect("save"), before, "{path}");
    }
}

#[test]
fn an_old_file_keeps_its_one_companion_and_gains_no_key() {
    let (_, text) = fixtures("companion-1.2.2").remove(0);
    let c = Character::from_json_str(&text).expect("load");
    assert_eq!(c.details.companion_name, "Shadow");
    assert!(c.extra["ui"]["companionPortrait"].is_string());
    let saved: Value = serde_json::from_str(&c.to_json_string().expect("save")).expect("json");
    assert!(saved.get("companions").is_none());
}

fn full() -> Companions {
    let granted = CompanionEdits {
        notes: "Sleeps by the fire.".into(),
        traits: vec![CompanionTrait {
            name: "Trained Nose".into(),
            text: "Advantage on checks to track by scent.".into(),
        }],
        armor_class_bonus: 1,
        hit_points_bonus: 0,
        speed_bonus: 10,
    };
    let owl = AddedCompanion {
        id: "companion-1".into(),
        creature: "ID_SRD51_MONSTER_OWL".into(),
        name: "Pip".into(),
        portrait: Some("data:image/png;base64,iVBORw0KGgo=".into()),
        edits: CompanionEdits {
            hit_points_bonus: -1,
            ..CompanionEdits::default()
        },
        extra: BTreeMap::from([("from_a_later_version".to_string(), json!([1, 2]))]),
    };
    let bare = AddedCompanion {
        id: "companion-2".into(),
        ..AddedCompanion::default()
    };
    Companions {
        granted,
        added: vec![owl, bare],
        extra: BTreeMap::from([("later".to_string(), json!({"x": true}))]),
    }
}

#[test]
fn edits_and_added_companions_survive_save_and_load() {
    let (_, text) = fixtures("companion-1.2.2").remove(0);
    let mut c = Character::from_json_str(&text).expect("load");
    c.companions = full();
    let saved = c.to_json_string().expect("save");
    let back = Character::from_json_str(&saved).expect("reload");
    assert_eq!(back, c);
    assert_eq!(back.to_json_string().expect("save again"), saved);
}

#[test]
fn the_saved_block_is_small_and_readable() {
    let mut c = Character::new("Wren");
    c.companions = full();
    let v: Value = serde_json::from_str(&c.to_json_string().expect("save")).expect("json");
    assert_eq!(
        v["companions"],
        json!({
            "granted": {"notes": "Sleeps by the fire.", "traits": [{"name": "Trained Nose",
                "text": "Advantage on checks to track by scent."}], "armor_class_bonus": 1, "speed_bonus": 10},
            "added": [
                {"id": "companion-1", "creature": "ID_SRD51_MONSTER_OWL", "name": "Pip",
                 "portrait": "data:image/png;base64,iVBORw0KGgo=", "hit_points_bonus": -1,
                 "from_a_later_version": [1, 2]},
                {"id": "companion-2"}
            ],
            "later": {"x": true}
        })
    );
}

#[test]
fn a_broken_companions_block_never_stops_the_character_loading() {
    for bad in [
        json!(7),
        json!("x"),
        json!({"added": 3}),
        json!({"added": [null, 1, {"id": 5}]}),
    ] {
        let text =
            json!({"format": "borealis-character", "format_version": 2, "ruleset": "dnd2014",
            "build": {"ruleset": "dnd2014", "levels": [{"kind": "main"}]}, "companions": bad})
            .to_string();
        let c = Character::from_json_str(&text).expect("loads");
        assert!(c.companions.added.len() <= 1, "{bad}");
    }
}

#[test]
fn the_aurora_import_of_a_companion_is_unchanged() {
    let Some(engine) = common::engine() else {
        eprintln!("SKIPPED: fixtures/aurora-xml not found");
        return;
    };
    let xml = EXAMPLE.replace("<companion name=\"\">", "<companion name=\"Shadow\">");
    let out = import_dnd5e(engine, &xml).expect("import");
    let c = out.character;
    assert_eq!(c.details.companion_name, "Shadow");
    assert!(c.companions.is_empty());
    let saved = c.to_json_string().expect("save");
    assert!(!saved.contains("\"companions\""));
    assert_eq!(Character::from_json_str(&saved).expect("reload"), c);
}
