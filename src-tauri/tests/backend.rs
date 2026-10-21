//! Backend service tests: they drive the same `AppState` methods the Tauri commands call, with a
//! temporary app data dir and the bundled content packs.

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::{ApiError, ErrorCode};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::PathBuf;

const HUMAN: &str = "ID_RACE_HUMAN";
const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

fn request(name: &str, ruleset: Ruleset, method: AbilityMethodDto) -> NewCharacterRequest {
    NewCharacterRequest {
        name: name.into(),
        ruleset,
        starting_level: 1,
        gender: "f".into(),
        ability_method: method,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    }
}

fn new_char(app: &AppState, method: AbilityMethodDto) -> CharacterSnapshot {
    app.create_character(Ruleset::Dnd2014, &request("Mira", Ruleset::Dnd2014, method))
        .expect("create")
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

fn code<T: std::fmt::Debug>(r: Result<T, ApiError>) -> ErrorCode {
    r.expect_err("expected an error").code
}

fn build(app: &AppState) -> CharacterSnapshot {
    let snap = new_char(app, AbilityMethodDto::PointBuy);
    let id = snap.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&snap, "Race"), HUMAN)
        .expect("race");
    app.apply_selection(&id, &slot_id(&s, "Class"), FIGHTER)
        .expect("class")
}

#[test]
fn create_select_level_save_reopen_round_trip() {
    let (_dir, app) = app();
    let snap = build(&app);
    let id = snap.id.clone();
    assert_eq!(snap.ruleset, Ruleset::Dnd2014);
    assert_eq!(snap.level.classes[0].name.as_deref(), Some("Fighter"));
    assert_eq!(
        snap.abilities.point_buy.as_ref().expect("point buy").budget,
        27
    );

    let up = app.level_up(&id, "main").expect("level up");
    assert_eq!(up.level.total, 2);
    assert!(up.dirty);
    assert!(up.stats.hp_max > snap.stats.hp_max);

    let saved = app.save_character(&id).expect("save");
    assert!(!saved.dirty);
    app.close_character(&id, false).expect("close");
    assert!(
        app.get_snapshot(&id).is_err(),
        "closed characters are not open"
    );

    let reopened = app.load_character(&id).expect("reopen");
    assert!(!reopened.dirty);
    assert_eq!(reopened.level.total, 2);
    assert_eq!(reopened.level.classes[0].name.as_deref(), Some("Fighter"));
    assert_eq!(reopened.stats.hp_max, up.stats.hp_max);
    assert_eq!(reopened.name, "Mira");
    let race = reopened
        .slots
        .iter()
        .find(|s| s.title == "Race")
        .expect("race slot");
    assert_eq!(race.chosen.as_ref().map(|c| c.id.as_str()), Some(HUMAN));

    let listed = app.list_characters().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].level, 2);
    assert!(listed[0].build.contains("Fighter 2"), "{}", listed[0].build);
}

#[test]
fn level_down_and_xp_follow_the_contract() {
    let (_dir, app) = app();
    let id = build(&app).id;
    let up = app.level_up(&id, "main").expect("up");
    assert_eq!(up.level.xp, 300, "level up raises XP to the threshold");
    let down = app.level_down(&id).expect("down");
    assert_eq!(down.level.total, 1);
    assert_eq!(down.level.xp, 0, "XP drops to the level threshold");
    assert_eq!(code(app.level_down(&id)), ErrorCode::RequirementFailed);
    let xp = app.set_xp(&id, 900).expect("xp");
    assert_eq!(xp.level.total, 1, "XP never auto-levels");
    assert!(xp.level.level_up_available);
    assert_eq!(code(app.set_xp(&id, -5)), ErrorCode::InvalidArgument);
}

#[test]
fn slot_options_filter_and_page() {
    let (_dir, app) = app();
    let snap = new_char(&app, AbilityMethodDto::PointBuy);
    let slot = slot_id(&snap, "Class");
    let all = app
        .get_slot_options(&snap.id, &slot, None, None, None)
        .expect("options");
    assert!(all.total >= 12);
    let one = app
        .get_slot_options(&snap.id, &slot, Some("wizard"), Some(5), Some(0))
        .expect("q");
    assert_eq!(one.total, 1);
    assert_eq!(one.items[0].summary.name, "Wizard");
    let page = app
        .get_slot_options(&snap.id, &slot, None, Some(3), Some(2))
        .expect("page");
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.total, all.total);
}

#[test]
fn clear_selection_empties_the_slot() {
    let (_dir, app) = app();
    let snap = build(&app);
    let slot = slot_id(&snap, "Class");
    let cleared = app.clear_selection(&snap.id, &slot).expect("clear");
    let class = cleared
        .slots
        .iter()
        .find(|s| s.title == "Class")
        .expect("slot");
    assert!(class.chosen.is_none());
}

#[test]
fn errors_map_to_codes_and_serialise_as_code_message() {
    let (_dir, app) = app();
    assert_eq!(code(app.get_snapshot("nope")), ErrorCode::NotFound);
    assert_eq!(code(app.load_character("missing")), ErrorCode::NotFound);
    assert_eq!(
        code(app.load_character("../etc/passwd")),
        ErrorCode::InvalidArgument
    );
    assert_eq!(code(app.delete_character("missing")), ErrorCode::NotFound);

    let snap = new_char(&app, AbilityMethodDto::PointBuy);
    let id = snap.id.clone();
    assert_eq!(
        code(app.apply_selection(&id, "garbage", HUMAN)),
        ErrorCode::InvalidArgument
    );
    let class = slot_id(&snap, "Class");
    assert_eq!(
        code(app.apply_selection(&id, &class, "ID_NO_SUCH")),
        ErrorCode::NotFound
    );
    assert_eq!(
        code(app.apply_selection(&id, &class, HUMAN)),
        ErrorCode::RequirementFailed
    );
    assert_eq!(
        code(app.level_up(&id, "multi-x")),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.create_character(
            Ruleset::Any,
            &request("X", Ruleset::Any, AbilityMethodDto::Manual)
        )),
        ErrorCode::InvalidArgument
    );

    let err = app.get_snapshot("nope").unwrap_err();
    let value = serde_json::to_value(&err).expect("json");
    assert_eq!(value["code"], "not_found");
    assert!(value["message"].is_string());
    assert!(value.get("details").is_none());
}

#[test]
fn failed_mutation_leaves_the_character_unchanged() {
    let (_dir, app) = app();
    let snap = build(&app);
    let before = app.get_snapshot(&snap.id).expect("snapshot");
    assert!(app
        .apply_selection(&snap.id, &slot_id(&snap, "Class"), HUMAN)
        .is_err());
    assert!(app.set_base_score(&snap.id, "str", 99).is_err());
    assert_eq!(app.get_snapshot(&snap.id).expect("after"), before);
}

#[test]
fn point_buy_budget_and_range_are_enforced() {
    let (_dir, app) = app();
    let snap = new_char(&app, AbilityMethodDto::PointBuy);
    let id = snap.id;
    let pb = snap.abilities.point_buy.expect("point buy");
    assert_eq!((pb.min, pb.max, pb.remaining), (8, 15, 27));
    assert_eq!(
        code(app.set_base_score(&id, "str", 16)),
        ErrorCode::InvalidArgument
    );
    for a in ["str", "dex", "con"] {
        app.set_base_score(&id, a, 15).expect("15 costs 9");
    }
    assert_eq!(
        code(app.set_base_score(&id, "int", 15)),
        ErrorCode::BudgetExceeded
    );
    let ok = app.set_base_score(&id, "int", 8).expect("8 is free");
    assert_eq!(ok.abilities.point_buy.expect("pb").remaining, 0);
    let unrestricted = app
        .set_character_options(
            &id,
            &OptionsPatch {
                point_buy_unrestricted: Some(true),
                ..OptionsPatch::default()
            },
        )
        .expect("options");
    assert!(unrestricted.options.point_buy_unrestricted);
    assert_eq!(
        code(app.set_base_score(&id, "wis", 17)),
        ErrorCode::BudgetExceeded
    );
}

#[test]
fn standard_array_assigns_swaps_and_unassigns() {
    let (_dir, app) = app();
    let id = new_char(&app, AbilityMethodDto::StandardArray).id;
    let s = app
        .assign_standard_array(&id, "str", Some(8))
        .expect("swap");
    let sa = s.abilities.standard_array.expect("array");
    assert_eq!(sa.assigned.str, Some(8));
    assert_eq!(
        sa.assigned.cha,
        Some(15),
        "the previous holder gets the old value"
    );
    let s = app
        .assign_standard_array(&id, "str", None)
        .expect("unassign");
    let sa = s.abilities.standard_array.expect("array");
    assert_eq!(sa.assigned.str, None);
    assert_eq!(sa.unassigned, vec![8]);
    assert_eq!(s.abilities.base.str, 8);
    let s = app
        .assign_standard_array(&id, "dex", None)
        .expect("unassign dex");
    let sa = s.abilities.standard_array.expect("array");
    assert_eq!((sa.assigned.str, sa.assigned.dex), (None, None));
    assert_eq!(sa.unassigned, vec![14, 8]);
    assert_eq!(
        code(app.assign_standard_array(&id, "str", Some(9))),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.roll_abilities(&id, None)),
        ErrorCode::RequirementFailed
    );
}

#[test]
fn rolling_records_history_and_stays_in_range() {
    let (_dir, app) = app();
    let id = new_char(&app, AbilityMethodDto::Roll4d6).id;
    let all = app.roll_abilities(&id, None).expect("roll all");
    assert_eq!(all.abilities.rolls.len(), 6);
    assert!(all
        .abilities
        .rolls
        .iter()
        .all(|r| r.dice.len() == 4 && (3..=18).contains(&r.total)));
    let one = app.roll_abilities(&id, Some("dex")).expect("roll one");
    assert_eq!(one.abilities.rolls.len(), 7);
    assert_eq!(one.abilities.rolls[6].ability, "dex");
    assert_eq!(
        one.abilities.base.dex,
        i32::try_from(one.abilities.rolls[6].total).unwrap()
    );
    let reset = app.reset_abilities(&id).expect("reset");
    assert!(reset.abilities.rolls.is_empty());
    assert_eq!(reset.abilities.base.str, 10);
}

#[test]
fn hp_rolls_multiclass_and_options() {
    let (_dir, app) = app();
    let id = build(&app).id;
    let up = app.level_up(&id, "main").expect("up");
    let die = up.level.classes[0].hit_die;
    assert_eq!(die, 10);
    assert_eq!(
        code(app.set_hp_roll(&id, "main", 2, die + 1)),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.set_hp_roll(&id, "main", 9, 3)),
        ErrorCode::InvalidArgument
    );
    let rolled = app.set_hp_roll(&id, "main", 2, 3).expect("roll");
    assert_eq!(rolled.level.classes[0].hp_rolls[1], 3);
    let off = app
        .set_character_options(
            &id,
            &OptionsPatch {
                multiclassing: Some(false),
                ..OptionsPatch::default()
            },
        )
        .expect("options");
    assert!(!off.options.multiclassing);
    assert!(!off.level.can_multiclass);
    assert_eq!(code(app.add_multiclass(&id)), ErrorCode::RequirementFailed);
}

#[test]
fn library_duplicate_meta_delete_and_export() {
    let (_dir, app) = app();
    let snap = build(&app);
    let id = snap.id.clone();
    let meta = app
        .update_character_meta(&id, Some(" Mira II "), Some("Party"), Some(true))
        .expect("meta");
    assert_eq!(
        (meta.name.as_str(), meta.group.as_str(), meta.favorite),
        ("Mira II", "Party", true)
    );
    assert_eq!(
        code(app.update_character_meta(&id, Some("  "), None, None)),
        ErrorCode::InvalidArgument
    );
    app.save_character(&id).expect("save");

    let copy = app.duplicate_character(&id).expect("dup");
    assert_eq!(copy.name, "Mira II (copy)");
    assert_ne!(copy.id, id);
    assert_eq!(app.list_characters().expect("list").len(), 2);

    let export = app.export_character(&id).expect("export");
    assert!(export.file_name.ends_with(".borealis.json"));
    assert!(export.data_base64.is_some());

    app.delete_character(&copy.id).expect("delete");
    assert_eq!(app.list_characters().expect("list").len(), 1);

    let portrait = app
        .set_portrait(&id, Some("data:image/png;base64,iVBORw0KGgo="))
        .expect("portrait");
    assert!(portrait
        .portrait
        .expect("portrait")
        .starts_with("data:image/png;base64,"));
    assert_eq!(
        code(app.set_portrait(&id, Some("http://x/y.png"))),
        ErrorCode::InvalidArgument
    );
    assert!(app
        .set_portrait(&id, None)
        .expect("clear")
        .portrait
        .is_none());
}

#[test]
fn save_as_creates_a_new_library_entry() {
    let (_dir, app) = app();
    let snap = build(&app);
    let copy = app
        .save_character_as(&snap.id, Some("Mira Twin"), None)
        .expect("save as");
    assert_ne!(copy.id, snap.id);
    assert_eq!(copy.name, "Mira Twin");
    assert!(!copy.dirty);
    assert_eq!(app.list_characters().expect("list").len(), 2);
}

#[test]
fn native_json_import_round_trips_and_bad_files_are_rejected() {
    let (_dir, app) = app();
    let id = build(&app).id;
    let export = app.export_character(&id).expect("export");
    let report = app
        .import_character("mira.json", export.data_base64.as_deref().expect("data"))
        .expect("import");
    assert_eq!(report.summary.name, "Mira");
    assert_eq!(app.list_characters().expect("list").len(), 2);
    assert_eq!(
        code(app.import_character("x.json", "!!!")),
        ErrorCode::InvalidArgument
    );
    use base64::Engine as _;
    let junk = base64::engine::general_purpose::STANDARD.encode("{\"hello\": 1}");
    assert_eq!(
        code(app.import_character("x.json", &junk)),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn legacy_dnd5e_import_returns_a_report() {
    let (_dir, app) = app();
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-character/tests/data/example.dnd5e");
    let Ok(xml) = std::fs::read(&sample) else {
        eprintln!("SKIPPED: example.dnd5e not present");
        return;
    };
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(xml);
    let report = app.import_character("example.dnd5e", &b64).expect("import");
    assert!(!report.summary.id.is_empty());
    assert!(
        !report.missing_content.is_empty() && report.summary.missing_content > 0,
        "SRD-only content cannot resolve every legacy element: {:?}",
        report.missing_content
    );
    assert!(app.load_character(&report.summary.id).is_ok());
}

#[test]
fn settings_patch_merges_validates_and_persists() {
    let (dir, app) = app();
    let s = app.get_settings().expect("settings");
    assert_eq!(s.point_buy_budget, 27);
    let patched = app
        .save_settings(
            &json!({"playerName": "Ada", "sheet": {"formFillable": true}, "theme": "dark"}),
        )
        .expect("patch");
    assert_eq!(patched.player_name, "Ada");
    assert!(patched.sheet.form_fillable);
    assert!(
        patched.sheet.style_formatting,
        "untouched nested fields survive"
    );
    assert_eq!(
        code(app.save_settings(&json!({"theme": "neon"}))),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.save_settings(&json!({"pointBuyBudget": 0}))),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.save_settings(&json!({"abilityMinBase": 20, "abilityMaxBase": 10}))),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.save_settings(&json!("nope"))),
        ErrorCode::InvalidArgument
    );

    let fresh = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    })
    .expect("restart");
    assert_eq!(fresh.get_settings().expect("reload").player_name, "Ada");
}

#[test]
fn engine_settings_changes_rebuild_the_engine() {
    let (_dir, app) = app();
    app.save_settings(&json!({"pointBuyBudget": 30}))
        .expect("budget");
    let snap = new_char(&app, AbilityMethodDto::PointBuy);
    assert_eq!(snap.abilities.point_buy.expect("pb").budget, 30);
}

#[test]
fn snapshot_json_matches_the_ui_contract_shape() {
    let (_dir, app) = app();
    let snap = build(&app);
    let v = serde_json::to_value(&snap).expect("json");
    for key in [
        "id",
        "name",
        "ruleset",
        "portrait",
        "group",
        "favorite",
        "dirty",
        "displayPath",
        "level",
        "abilities",
        "stats",
        "slots",
        "progress",
        "spellcasting",
        "inventory",
        "details",
        "attacks",
        "companion",
        "options",
        "availableOptions",
        "sourceRestrictions",
        "notices",
    ] {
        assert!(v.get(key).is_some(), "missing snapshot field {key}");
    }
    assert_eq!(v["ruleset"], "dnd2014");
    assert!(v["level"]["canLevelUp"].is_boolean());
    assert!(v["abilities"]["final"]["str"].is_number());
    assert_eq!(v["abilities"]["method"], "point_buy");
    assert!(v["abilities"]["pointBuy"]["costs"]["15"].is_number());
    assert_eq!(v["stats"]["skills"].as_array().expect("skills").len(), 18);
    assert!(v["slots"][0]["elementType"].is_string());
    assert!(v["progress"]["byTab"]["race"].is_number());
    assert_eq!(v["progress"]["byTab"].as_object().expect("tabs").len(), 9);
    assert!(v["inventory"]["storageNames"].is_array());
    assert!(v["details"]["personalityTraits"].is_string());
}

#[test]
fn ruleset_2024_characters_can_be_created() {
    let (_dir, app) = app();
    let snap = app
        .create_character(
            Ruleset::Dnd2024,
            &request("Ona", Ruleset::Dnd2024, AbilityMethodDto::PointBuy),
        )
        .expect("2024");
    assert_eq!(snap.ruleset, Ruleset::Dnd2024);
    assert!(snap.slots.iter().any(|s| s.tab == "race"));
}

#[test]
fn starting_level_applies_extra_levels() {
    let (_dir, app) = app();
    let mut req = request("Vet", Ruleset::Dnd2014, AbilityMethodDto::Manual);
    req.starting_level = 5;
    let snap = app
        .create_character(Ruleset::Dnd2014, &req)
        .expect("create");
    assert_eq!(snap.level.total, 5);
    assert_eq!(snap.level.xp, 6500);
}

#[test]
fn slots_are_placed_on_the_right_tabs_and_a_sole_candidate_is_left_to_the_user() {
    let (_dir, app) = app();
    let snap = build(&app);
    let bg = snap
        .slots
        .iter()
        .find(|s| s.title == "Background")
        .expect("background slot");
    // engine-optional-sole-option SO-1: Acolyte is the only SRD background, but without a
    // `default` Aurora leaves the slot empty and unlocked
    assert!(bg.chosen.is_none() && !bg.locked && bg.required, "{bg:?}");
    let snap = app
        .apply_selection(&snap.id, &bg.id, "ID_BACKGROUND_ACOLYTE")
        .expect("acolyte");
    for title in ["Ideal", "Bond", "Flaw"] {
        let slot = snap.slots.iter().find(|s| s.title == title).expect("slot");
        assert_eq!(slot.tab, "background", "{title}");
    }
    assert!(snap.slots.iter().any(|s| s.tab == "languages"));
    assert_eq!(snap.progress.total, snap.slots.len());
    assert!(snap.progress.next_slot_id.is_some());
    let reason = snap
        .level
        .multiclass_blocked_reason
        .expect("blocked at 9 STR/DEX");
    assert!(reason.contains("Strength 13"), "{reason}");
}

/// engine-optional-sole-option SO-2/SO-3/SO-4: a slot's `default` is chosen when the slot first
/// appears (the 2024 Fighter's level 4 feat defaults to Ability Score Improvement). While it is
/// the only qualifying feat (all scores 8) the slot is locked and cannot be cleared; once
/// Strength 15 lets other feats qualify it unlocks and keeps its choice; a cleared default is
/// not re-applied by later commands.
#[test]
fn a_default_is_chosen_when_its_slot_appears_and_locks_while_it_is_the_sole_option() {
    const FEAT_SLOT: &str = "Ability Score Improvement (Fighter 4)";
    const ASI: &str = "ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT";
    let (_dir, app) = app();
    let snap = app
        .create_character(
            Ruleset::Dnd2024,
            &request("Ona", Ruleset::Dnd2024, AbilityMethodDto::Manual),
        )
        .expect("2024");
    let id = snap.id.clone();
    for a in ["str", "dex", "con", "int", "wis", "cha"] {
        app.set_base_score(&id, a, 8).expect("score");
    }
    let mut s = app
        .apply_selection(&id, &slot_id(&snap, "Class"), "ID_2024_CLASS_FIGHTER")
        .expect("class");
    assert!(!s.slots.iter().any(|x| x.title == FEAT_SLOT));
    while s.level.total < 4 {
        s = app.level_up(&id, "main").expect("level up");
    }
    let feat = s.slots.iter().find(|x| x.title == FEAT_SLOT).expect("feat");
    assert_eq!(feat.chosen.as_ref().map(|c| c.id.as_str()), Some(ASI));
    let feat_id = feat.id.clone();
    let offered = |app: &AppState| {
        app.get_slot_options(&id, &feat_id, None, Some(200), None)
            .expect("options")
            .items
            .into_iter()
            .filter(|o| o.disabled_reason.is_none())
            .count()
    };
    assert_eq!(offered(&app), 1, "only ASI qualifies with 8s");
    assert!(
        feat.locked,
        "a chosen default that is the sole option is locked"
    );
    assert_eq!(
        code(app.clear_selection(&id, &feat_id)),
        ErrorCode::InvalidArgument
    );
    let s = app.set_base_score(&id, "str", 15).expect("str 15");
    assert!(offered(&app) > 1);
    let feat = s.slots.iter().find(|x| x.id == feat_id).expect("feat");
    assert!(!feat.locked, "a grown list unlocks the slot");
    assert_eq!(feat.chosen.as_ref().map(|c| c.id.as_str()), Some(ASI));
    app.clear_selection(&id, &feat_id).expect("clear");
    let s = app.level_up(&id, "main").expect("level 5");
    let feat = s.slots.iter().find(|x| x.id == feat_id).expect("feat");
    assert!(feat.chosen.is_none(), "a cleared default is not re-applied");
}
