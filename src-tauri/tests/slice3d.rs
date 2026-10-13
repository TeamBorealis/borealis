//! 2024 play-state commands, DTO blocks and "Convert to 2024" through `AppState`.

use borealis_character::Character;
use borealis_content::Ruleset;
use borealis_engine::commands::{self, slot_key, Improvement};
use borealis_engine::state::AbilityMethod;
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::dto::{CharacterSnapshot, Ruleset as DtoRuleset};
use borealis_lib::error::ErrorCode;
use borealis_lib::session::new_id;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::Value;
use std::path::PathBuf;

const GREATSWORD: &str = "ID_2024_WEAPON_GREATSWORD";

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn level_to(e: &Engine, mut s: CharacterState, level: u32) -> CharacterState {
    while s.level() < level {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    s
}

fn open(app: &AppState, name: &str, build: CharacterState) -> String {
    let mut c = Character::new(name);
    c.build = build;
    let id = new_id();
    let path = app.character_path(&id).unwrap();
    c.save(&path).unwrap();
    app.insert_session(&id, c, path, false).unwrap();
    id
}

/// A 2024 Dwarf / Acolyte of `class` at `level`, optionally carrying a greatsword.
fn hero_2024(app: &AppState, class: &str, class_name: &str, level: u32, sword: bool) -> String {
    let hub = app.hub().unwrap();
    let e = &hub.engine;
    let array = e
        .universe(Ruleset::Dnd2024)
        .tables()
        .standard_array_for(class_name)
        .expect("class array");
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    s = pick(e, &s, "ID_LEVEL_1", "Race", "ID_2024_SPECIES_DWARF");
    s = pick(e, &s, "ID_LEVEL_1", "Class", class);
    s = pick(
        e,
        &s,
        "ID_LEVEL_1",
        "Background",
        "ID_2024_BACKGROUND_ACOLYTE",
    );
    s = commands::set_ability_method(&s, AbilityMethod::StandardArray);
    s = commands::set_base_scores(e, &s, array).unwrap().state;
    let snap = e.derive(&s);
    let slot = snap
        .slots
        .iter()
        .find(|x| {
            x.owner.id == "ID_2024_BACKGROUND_ACOLYTE"
                && x.list_choice.is_none()
                && !x.list_items.is_empty()
        })
        .expect("ability slot");
    let item = slot
        .list_items
        .iter()
        .find(|(_, t)| t == "Wisdom +2, Charisma +1")
        .expect("entry")
        .0;
    s = commands::select(e, &s, &slot.key.clone(), Choice::ListItem(item))
        .unwrap()
        .state;
    s = level_to(e, s, level);
    if sword {
        s = commands::add_item(e, &s, GREATSWORD, 1).unwrap().state;
        let uid = s.inventory.items.last().unwrap().uid.clone();
        s = commands::equip(e, &s, &uid, None).unwrap().state;
    }
    open(app, "Hero", s)
}

fn resource(snap: &CharacterSnapshot, id: &str) -> (i64, i64) {
    let r = snap
        .play
        .resources
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("resource {id}"));
    (r["current"].as_i64().unwrap(), r["max"].as_i64().unwrap())
}

fn active_resistances(snap: &CharacterSnapshot) -> Vec<String> {
    snap.play.defenses["resistances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["active"] == Value::Bool(true))
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect()
}

fn hill_dwarf_cleric(app: &AppState) -> String {
    let hub = app.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    s = pick(e, &s, "ID_LEVEL_1", "Race", "ID_SRD_RACE_DWARF");
    s = pick(e, &s, "ID_LEVEL_1", "Class", "ID_WOTC_PHB_CLASS_CLERIC");
    s = pick(e, &s, "ID_LEVEL_1", "Background", "ID_BACKGROUND_ACOLYTE");
    s = level_to(e, s, 5);
    s = pick(
        e,
        &s,
        "ID_RACIAL_TRAIT_DWARVEN_SUBRACE",
        "Dwarven Subrace",
        "ID_SUB_RACE_HILL_DWARF",
    );
    s = pick(
        e,
        &s,
        "ID_WOTC_PHB_CLASS_FEATURE_CLERIC_DIVINEDOMAIN",
        "Divine Domain",
        "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN",
    );
    s = commands::choose_improvement(e, &s, "Cleric", 4, &Improvement::PlusTwo(4))
        .unwrap()
        .state;
    open(app, "Brunhild", s)
}

#[test]
fn convert_preview_and_apply_leave_the_original_untouched() {
    let (_dir, app) = app();
    let id = hill_dwarf_cleric(&app);
    let original = std::fs::read(app.character_path(&id).unwrap()).unwrap();

    let preview = app.preview_convert_to_2024(&id).unwrap();
    assert_eq!(preview.character_id, id);
    assert_eq!((preview.before.level, preview.after.level), (5, 5));
    assert!(preview.name.ends_with("(2024)"));
    assert_eq!(
        preview.counts.mapped,
        preview
            .items
            .iter()
            .filter(|i| i.outcome == "mapped")
            .count()
    );
    let class = preview
        .items
        .iter()
        .find(|i| i.category == "class" && i.label.contains("Cleric"))
        .expect("class row");
    assert_eq!(class.outcome, "mapped");
    assert_eq!(class.to_id.as_deref(), Some("ID_2024_CLASS_CLERIC"));
    assert!(preview
        .items
        .iter()
        .any(|i| i.outcome == "dropped" && i.reason.is_some()));
    assert!(preview
        .items
        .iter()
        .any(|i| i.outcome == "needs_choice" && i.what.is_some()));
    assert_eq!(
        std::fs::read(app.character_path(&id).unwrap()).unwrap(),
        original
    );

    let applied = app.apply_convert_to_2024(&id).unwrap();
    assert_ne!(applied.new_character_id, id);
    assert_eq!(applied.snapshot.id, applied.new_character_id);
    assert_eq!(applied.snapshot.ruleset, DtoRuleset::Dnd2024);
    assert!(applied.snapshot.name.ends_with("(2024)"));
    assert_eq!(applied.snapshot.level.total, 5);
    assert!(app
        .character_path(&applied.new_character_id)
        .unwrap()
        .is_file());
    assert_eq!(
        std::fs::read(app.character_path(&id).unwrap()).unwrap(),
        original
    );
    assert_eq!(app.get_snapshot(&id).unwrap().ruleset, DtoRuleset::Dnd2014);

    let again = app
        .preview_convert_to_2024(&applied.new_character_id)
        .unwrap_err();
    assert_eq!(again.code, ErrorCode::RequirementFailed);
}

#[test]
fn a_long_rest_restores_resources_and_lowers_exhaustion_by_one() {
    let (_dir, app) = app();
    let id = hero_2024(&app, "ID_2024_CLASS_BARBARIAN", "Barbarian", 5, false);
    let snap = app.spend_resource(&id, "rage", 2).unwrap();
    assert_eq!(resource(&snap, "rage"), (1, 3));
    assert_eq!(
        app.spend_resource(&id, "rage", 5).unwrap_err().code,
        ErrorCode::RequirementFailed
    );
    let snap = app.short_rest(&id).unwrap();
    assert_eq!(resource(&snap, "rage"), (2, 3));
    app.spend_resource(&id, "rage", 2).unwrap();
    let snap = app.set_exhaustion(&id, 3).unwrap();
    assert_eq!(snap.play.exhaustion["level"], 3);
    assert_eq!(snap.play.exhaustion["d20Penalty"], -6);
    assert_eq!(snap.play.speeds.walk, snap.stats.speed);
    let snap = app.long_rest(&id).unwrap();
    assert_eq!(resource(&snap, "rage"), (3, 3));
    assert_eq!(snap.play.exhaustion["level"], 2);
    let snap = app.set_heroic_inspiration(&id, true).unwrap();
    assert!(snap.play.inspiration);
}

#[test]
fn toggling_rage_changes_the_resistances_in_the_snapshot() {
    let (_dir, app) = app();
    let id = hero_2024(&app, "ID_2024_CLASS_BARBARIAN", "Barbarian", 5, false);
    let off = app.get_snapshot(&id).unwrap();
    assert!(active_resistances(&off).iter().all(|r| r != "bludgeoning"));
    assert!(off.play.defenses["resistances"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"] == "bludgeoning" && r["whileToggle"].is_string()));
    let on = app.set_toggle(&id, "rage", true).unwrap();
    for t in ["bludgeoning", "piercing", "slashing"] {
        assert!(active_resistances(&on).iter().any(|r| r == t), "{t}");
    }
    assert!(on
        .play
        .toggles
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == "rage" && t["active"] == true));
    let ended = app.short_rest(&id).unwrap();
    assert!(active_resistances(&ended)
        .iter()
        .all(|r| r != "bludgeoning"));
    assert_eq!(
        app.set_toggle(&id, "no-such-toggle", true)
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[test]
fn a_mastery_choice_shows_on_the_attack_and_features_carry_text() {
    let (_dir, app) = app();
    let id = hero_2024(&app, "ID_2024_CLASS_FIGHTER", "Fighter", 1, true);
    let before = app.get_snapshot(&id).unwrap();
    let sword = before
        .attacks
        .iter()
        .find(|a| a.name == "Greatsword")
        .unwrap();
    assert_eq!(
        sword
            .mastery
            .as_ref()
            .map(|m| (m.name.as_str(), m.mastered)),
        Some(("Graze", false))
    );
    assert_eq!(sword.crit_range, 20);

    let snap = app
        .choose_weapon_masteries(&id, &[GREATSWORD.to_string()])
        .unwrap();
    let sword = snap
        .attacks
        .iter()
        .find(|a| a.name == "Greatsword")
        .unwrap();
    assert!(sword.mastery.as_ref().is_some_and(|m| m.mastered));
    assert_eq!(snap.play.masteries[0]["weapon"], GREATSWORD);

    let dup = vec![GREATSWORD.to_string(), GREATSWORD.to_string()];
    assert_eq!(
        app.choose_weapon_masteries(&id, &dup).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
    let many: Vec<String> = (0..9).map(|i| format!("W{i}")).collect();
    assert_eq!(
        app.choose_weapon_masteries(&id, &many).unwrap_err().code,
        ErrorCode::RequirementFailed
    );
    let cleared = app.choose_weapon_masteries(&id, &[]).unwrap();
    assert!(cleared.play.masteries.as_array().unwrap().is_empty());

    let features = snap.play.features.as_array().unwrap();
    assert!(!features.is_empty());
    assert!(features
        .iter()
        .any(|f| f["description"].as_str().is_some_and(|d| !d.is_empty())));
    assert!(snap.play.compatibility["strict"].is_boolean());
}

#[test]
fn wild_shape_and_package_commands_are_refused_when_they_do_not_apply() {
    let (_dir, app) = app();
    let id = hero_2024(&app, "ID_2024_CLASS_FIGHTER", "Fighter", 1, false);
    assert_eq!(
        app.set_wild_shape_forms(&id, &["ID_X".into()])
            .unwrap_err()
            .code,
        ErrorCode::RequirementFailed
    );
    let snap = app.get_snapshot(&id).unwrap();
    let pkgs = snap.play.equipment_packages.as_array().unwrap();
    assert!(!pkgs.is_empty());
    let pkg = pkgs[0]["package"].as_str().unwrap().to_string();
    let after = app.choose_equipment_package(&id, &pkg, "A").unwrap();
    assert!(after.inventory.items.len() > snap.inventory.items.len());
    assert_eq!(
        app.choose_equipment_package(&id, &pkg, "A")
            .unwrap_err()
            .code,
        ErrorCode::RequirementFailed
    );
}
