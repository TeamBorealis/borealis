//! A hand-made attack can depend on an ability like a weapon attack (GitHub #29): its bonus is
//! the ability modifier plus the proficiency bonus and its damage the typed dice plus the
//! modifier, and both follow later score changes. Checked against the TS contract.

mod contract_support;
use borealis_lib::content_dto::AttackInput;
use borealis_lib::dto::{
    AbilityMethodDto, Attack, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use std::path::PathBuf;

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

/// A level 1 2024 hero (proficiency +2) with Strength 16.
fn hero(app: &AppState) -> String {
    let request = NewCharacterRequest {
        name: "Claw".into(),
        ruleset: Ruleset::Dnd2024,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let id = app
        .create_character(Ruleset::Dnd2024, &request)
        .expect("create")
        .id;
    app.set_base_score(&id, "str", 16).expect("str");
    id
}

fn bite(calculated: bool) -> AttackInput {
    AttackInput {
        name: "Bite".into(),
        range: "5 ft.".into(),
        attack_bonus: "+0".into(),
        damage: "1d6 piercing".into(),
        ability: Some("str".into()),
        show_in_combat: true,
        calculated,
        ..AttackInput::default()
    }
}

fn row<'a>(s: &'a CharacterSnapshot, name: &str) -> &'a Attack {
    s.attacks.iter().find(|a| a.name == name).expect(name)
}

#[test]
fn a_calculated_custom_attack_follows_its_ability() {
    let (_d, app) = app();
    let contract = Contract::load();
    let id = hero(&app);
    let s = app.upsert_attack(&id, &bite(true)).expect("bite");
    let v = serde_json::to_value(&s).expect("json");
    let errors = contract.response("upsert_attack", &v);
    assert!(errors.is_empty(), "{errors:?}");
    let b = row(&s, "Bite");
    assert!(b.calculated);
    assert!(!b.auto, "still a hand-made row");
    assert_eq!(b.attack_bonus, "+5");
    assert_eq!(b.damage, "1d6+3 piercing");
    assert_eq!(b.to_hit, Some(5));

    // a higher Strength raises it; another ability replaces the modifier
    let s = app.set_base_score(&id, "str", 18).expect("str 18");
    assert_eq!(row(&s, "Bite").attack_bonus, "+6");
    let mut dex = bite(true);
    dex.id = Some(b.id.clone());
    dex.ability = Some("dex".into());
    dex.damage = row(&s, "Bite").damage.clone();
    let s = app.upsert_attack(&id, &dex).expect("dex bite");
    assert_eq!(row(&s, "Bite").attack_bonus, "+2");
    assert_eq!(row(&s, "Bite").damage, "1d6 piercing");

    app.save_character(&id).expect("save");
    app.close_character(&id, false).expect("close");
    let back = app.load_character(&id).expect("reopen");
    assert!(row(&back, "Bite").calculated);
    assert_eq!(row(&back, "Bite").ability.as_deref(), Some("dex"));
}

#[test]
fn a_typed_custom_attack_keeps_its_texts() {
    let (_d, app) = app();
    let id = hero(&app);
    let s = app.upsert_attack(&id, &bite(false)).expect("bite");
    let b = row(&s, "Bite");
    assert!(!b.calculated);
    assert_eq!(b.attack_bonus, "+0");
    assert_eq!(b.damage, "1d6 piercing");
}
