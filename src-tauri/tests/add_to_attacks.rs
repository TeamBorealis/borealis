//! Aurora's "Add to Attacks & Spellcasting" on a weapon (GitHub #28): the weapon's row stays in the
//! attacks while it is not held, survives a reopen, and Remove takes it off the list again. A held
//! weapon's row cannot be removed (it is hidden from the sheet instead). Checked against the TS
//! contract.

mod contract_support;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::EquipRequest;
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use std::path::PathBuf;

const DAGGER: &str = "ID_2024_WEAPON_DAGGER";
const SHIELD: &str = "ID_2024_ARMOR_SHIELD";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn hero(app: &AppState) -> String {
    let request = NewCharacterRequest {
        name: "Lister".into(),
        ruleset: Ruleset::Dnd2024,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    app.create_character(Ruleset::Dnd2024, &request)
        .expect("create")
        .id
}

fn add(app: &AppState, id: &str, item: &str) -> String {
    let s = app.add_item(id, item, 1, false, None).expect("item");
    s.inventory.items.last().expect("line").id.clone()
}

fn has_row(s: &CharacterSnapshot, uid: &str) -> bool {
    s.attacks.iter().any(|a| a.id == uid)
}

#[test]
fn an_added_weapon_keeps_its_row_until_it_is_removed() {
    let (_d, app) = app();
    let contract = Contract::load();
    let id = hero(&app);
    let dagger = add(&app, &id, DAGGER);
    assert!(!has_row(&app.get_snapshot(&id).expect("snap"), &dagger));

    let s = app.add_to_attacks(&id, &dagger).expect("add to attacks");
    let errors = contract.response("add_to_attacks", &serde_json::to_value(&s).expect("json"));
    assert!(errors.is_empty(), "{errors:?}");
    assert!(has_row(&s, &dagger), "the carried dagger is in the attacks");
    let again = app.add_to_attacks(&id, &dagger).expect("twice");
    assert_eq!(
        again.attacks.iter().filter(|a| a.id == dagger).count(),
        1,
        "added twice, listed once"
    );

    app.save_character(&id).expect("save");
    app.close_character(&id, false).expect("close");
    let back = app.load_character(&id).expect("reopen");
    assert!(has_row(&back, &dagger), "the list is saved");

    let s = app.remove_attack(&id, &dagger).expect("remove");
    assert!(!has_row(&s, &dagger));
}

#[test]
fn only_a_listed_row_can_be_removed_and_only_weapons_can_be_added() {
    let (_d, app) = app();
    let id = hero(&app);
    let dagger = add(&app, &id, DAGGER);
    app.equip_item(&id, &dagger, EquipRequest::Primary)
        .expect("equip");
    let err = app.remove_attack(&id, &dagger).expect_err("held");
    assert_eq!(err.code, ErrorCode::RequirementFailed);
    // once listed, removing takes it off the list and the held weapon keeps its row
    app.add_to_attacks(&id, &dagger).expect("add");
    let s = app.remove_attack(&id, &dagger).expect("unlist");
    assert!(has_row(&s, &dagger));

    let shield = add(&app, &id, SHIELD);
    assert!(app.add_to_attacks(&id, &shield).is_err());
    assert!(app.add_to_attacks(&id, "item-999").is_err());
}
