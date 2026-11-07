//! HR3 (GitHub #24) on the real backend: the "Ignore feat prerequisites" and "Ignore multiclass
//! prerequisites" house rules are character options, mark the picks they allow (picker rows,
//! the slot, the class list), survive save and reopen, and turning one off keeps the pick as an
//! invalid one with a notice instead of deleting it.

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const BARD: &str = "ID_WOTC_PHB_MULTICLASS_BARD";
const FIGHTER: &str = "ID_WOTC_PHB_MULTICLASS_FIGHTER";
const OWN: &str = "ID_WOTC_PHB_MULTICLASS_WIZARD";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

fn rule(feats: Option<bool>, multiclass: Option<bool>) -> OptionsPatch {
    OptionsPatch {
        ignore_feat_prerequisites: feats,
        ignore_multiclass_prerequisites: multiclass,
        ..OptionsPatch::default()
    }
}

/// A 2014 elf Wizard 1 with STR 13 / INT 13 / CHA 8 and multiclassing on.
fn wizard(app: &AppState) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Seraphine".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch {
            multiclassing: Some(true),
            ..OptionsPatch::default()
        },
        portrait: None,
        group: None,
    };
    let s = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    let id = s.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&s, "Race"), "ID_RACE_ELF")
        .expect("race");
    let s = app
        .apply_selection(&id, &slot_id(&s, "Class"), "ID_WOTC_PHB_CLASS_WIZARD")
        .expect("class");
    app.apply_selection(&id, &slot_id(&s, "Background"), "ID_BACKGROUND_ACOLYTE")
        .expect("background");
    app.set_base_score(&id, "str", 13).expect("str");
    app.set_base_score(&id, "cha", 8).expect("cha");
    app.set_base_score(&id, "int", 13).expect("int")
}

#[test]
fn ignored_multiclass_prerequisites_mark_save_and_turn_off_honestly() {
    let (_dir, app) = app();
    let snap = wizard(&app);
    let id = snap.id.clone();
    assert!(!snap.options.ignore_feat_prerequisites);
    assert!(!snap.options.ignore_multiclass_prerequisites);

    let on = app
        .set_character_options(&id, &rule(None, Some(true)))
        .expect("house rule");
    assert!(on.options.ignore_multiclass_prerequisites);
    let snap = app.add_multiclass(&id).expect("new multiclass");
    let slot = slot_id(&snap, "Multiclass (Level 2)");
    let page = app
        .get_slot_options(&id, &slot, None, None, None)
        .expect("options");
    let rows: Vec<(String, bool)> = page
        .items
        .iter()
        .flat_map(|o| o.all_versions().iter())
        .map(|v| (v.summary.id.clone(), v.prerequisites_unmet))
        .collect();
    let mark = |id: &str| rows.iter().find(|(x, _)| x == id).map(|(_, m)| *m);
    assert_eq!(
        mark(BARD),
        Some(true),
        "CHA 8 Bard is offered, marked: {rows:?}"
    );
    assert_eq!(mark(FIGHTER), Some(false), "STR 13 Fighter is not marked");
    assert_eq!(mark(OWN), None, "the own class is never offered");

    let picked = app.apply_selection(&id, &slot, BARD).expect("pick Bard");
    let s = picked
        .slots
        .iter()
        .find(|s| s.id == slot)
        .expect("multiclass slot");
    assert!(s.prerequisites_unmet, "the chosen pick is marked");
    let bard = picked
        .level
        .classes
        .iter()
        .find(|c| c.name.as_deref() == Some("Bard"))
        .expect("Bard class");
    assert!(bard.prerequisites_unmet, "the class list marks it");
    assert!(picked
        .level
        .classes
        .iter()
        .all(|c| c.is_main != c.prerequisites_unmet));

    // the house rule and the pick survive save and reopen
    app.save_character(&id).expect("save");
    app.close_character(&id, false).expect("close");
    let back = app.load_character(&id).expect("reopen");
    assert!(back.options.ignore_multiclass_prerequisites);
    assert!(!back.options.ignore_feat_prerequisites);
    assert!(back
        .level
        .classes
        .iter()
        .any(|c| c.name.as_deref() == Some("Bard")));

    // off: the pick is not applied, a notice says why, nothing is deleted; on again restores it
    let off = app
        .set_character_options(&id, &rule(None, Some(false)))
        .expect("off");
    assert!(!off
        .level
        .classes
        .iter()
        .any(|c| c.name.as_deref() == Some("Bard")));
    assert!(
        off.notices
            .iter()
            .any(|n| n.contains("Bard") && n.contains("Ignore multiclass prerequisites")),
        "{:?}",
        off.notices
    );
    let again = app
        .set_character_options(&id, &rule(None, Some(true)))
        .expect("on again");
    assert!(again
        .level
        .classes
        .iter()
        .any(|c| c.name.as_deref() == Some("Bard")));
    assert!(again
        .notices
        .iter()
        .all(|n| !n.contains("Ignore multiclass")));
}

#[test]
fn ignore_feat_prerequisites_is_a_character_option() {
    let (_dir, app) = app();
    let snap = wizard(&app);
    let id = snap.id.clone();
    let on = app
        .set_character_options(&id, &rule(Some(true), None))
        .expect("on");
    assert!(on.options.ignore_feat_prerequisites);
    assert!(!on.options.ignore_multiclass_prerequisites);
    app.save_character(&id).expect("save");
    app.close_character(&id, false).expect("close");
    let back = app.load_character(&id).expect("reopen");
    assert!(back.options.ignore_feat_prerequisites);
}
