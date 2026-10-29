//! Play tab spell tracker (GitHub #2, decision U13) through the real command layer: spell slots
//! per level and Pact Magic slots from the character's computed slots, used counts recovered by
//! the existing rest commands and clamped when the maximum drops, concentration on a known or
//! prepared concentration spell, the user's own trackers, and all of it saved with the character
//! (old files load with nothing tracked). Every response is checked against the TS contract.

mod contract_support;
use borealis_engine::state::TrackerReset;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::path::PathBuf;

const BLESS: &str = "ID_PHB_SPELL_BLESS";
const CURE_WOUNDS: &str = "ID_PHB_SPELL_CURE_WOUNDS";

struct App {
    state: AppState,
    contract: Contract,
    _dir: tempfile::TempDir,
}

impl App {
    fn new() -> Self {
        borealis_engine::invariants::enable_for_thread();
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: dir.path().to_path_buf(),
        };
        Self {
            state: AppState::new(paths).expect("state"),
            contract: Contract::load(),
            _dir: dir,
        }
    }

    /// The snapshot as the UI receives it, after checking it against the TS response type.
    fn play(&self, command: &str, snap: &CharacterSnapshot) -> Value {
        let v = serde_json::to_value(snap).expect("json");
        let errors = self.contract.response(command, &v);
        assert!(errors.is_empty(), "{command}: {errors:?}");
        v["play"].clone()
    }

    fn create(&self, ruleset: Ruleset, race: &str, class: &str) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Tracker".into(),
            ruleset,
            starting_level: 1,
            gender: String::new(),
            // manual scores: the contract generator types point-buy `costs` (a number-keyed
            // record) as an empty object, which no real snapshot matches
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        let s = self
            .state
            .create_character(ruleset, &request)
            .expect("create");
        let s = self
            .state
            .apply_selection(&s.id, &slot_id(&s, "Race"), race)
            .expect("race");
        self.state
            .apply_selection(&s.id, &slot_id(&s, "Class"), class)
            .expect("class")
    }

    fn wizard(&self, level: u32) -> CharacterSnapshot {
        let mut s = self.create(
            Ruleset::Dnd2024,
            "ID_2024_SPECIES_ELF",
            "ID_2024_CLASS_WIZARD",
        );
        while s.level.total < level {
            s = self.state.level_up(&s.id, "main").expect("level up");
        }
        s
    }
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

fn slot_rows(play: &Value) -> Vec<(u64, u64, u64)> {
    play["spellSlots"]
        .as_array()
        .expect("spellSlots")
        .iter()
        .map(|r| {
            (
                r["level"].as_u64().unwrap(),
                r["max"].as_u64().unwrap(),
                r["used"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn slots_are_marked_used_and_come_back_on_the_existing_rests() {
    let app = App::new();
    let s = app.wizard(3);
    let id = s.id.clone();
    let play = app.play("get_snapshot", &s);
    assert_eq!(slot_rows(&play), vec![(1, 4, 0), (2, 2, 0)]);
    assert!(play["pactSlots"].is_null());

    let s = app
        .state
        .set_spell_slots_used(&id, 1, 3, false)
        .expect("use");
    assert_eq!(
        slot_rows(&app.play("set_spell_slots_used", &s)),
        vec![(1, 4, 3), (2, 2, 0)]
    );
    let s = app
        .state
        .set_spell_slots_used(&id, 2, 2, false)
        .expect("use");
    assert_eq!(
        slot_rows(&app.play("set_spell_slots_used", &s)),
        vec![(1, 4, 3), (2, 2, 2)]
    );
    let err = app
        .state
        .set_spell_slots_used(&id, 1, 5, false)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);
    let err = app.state.set_spell_slots_used(&id, 1, 0, true).unwrap_err();
    assert_eq!(
        err.code,
        ErrorCode::RequirementFailed,
        "a wizard has no pact slots"
    );

    let s = app.state.short_rest(&id).expect("short rest");
    assert_eq!(
        slot_rows(&app.play("short_rest", &s)),
        vec![(1, 4, 3), (2, 2, 2)]
    );
    let s = app.state.long_rest(&id).expect("long rest");
    assert_eq!(
        slot_rows(&app.play("long_rest", &s)),
        vec![(1, 4, 0), (2, 2, 0)]
    );
}

#[test]
fn a_level_down_clamps_the_saved_used_count() {
    let app = App::new();
    let s = app.wizard(3);
    let id = s.id.clone();
    app.state
        .set_spell_slots_used(&id, 1, 4, false)
        .expect("use");
    app.state
        .set_spell_slots_used(&id, 2, 2, false)
        .expect("use");
    let s = app.state.level_down(&id).expect("level down");
    assert_eq!(slot_rows(&app.play("level_down", &s)), vec![(1, 3, 3)]);
    app.state.save_character(&id).expect("save");
    let path = app.state.character_path(&id).expect("path");
    let text = std::fs::read_to_string(path).expect("file");
    let file: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(file["build"]["play"]["spell_slots_used"], json!({"1": 3}));
}

#[test]
fn a_warlock_has_a_pact_row_recovered_on_a_short_rest() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2014,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_WARLOCK",
    );
    let id = s.id.clone();
    let play = app.play("get_snapshot", &s);
    assert!(slot_rows(&play).is_empty());
    assert_eq!(play["pactSlots"], json!({"level": 1, "max": 1, "used": 0}));
    let s = app
        .state
        .set_spell_slots_used(&id, 1, 1, true)
        .expect("use pact");
    assert_eq!(app.play("set_spell_slots_used", &s)["pactSlots"]["used"], 1);
    let s = app.state.short_rest(&id).expect("short rest");
    assert_eq!(app.play("short_rest", &s)["pactSlots"]["used"], 0);
}

#[test]
fn concentration_is_picked_replaced_ended_and_cleared_on_a_long_rest() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2014,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_CLERIC",
    );
    let id = s.id.clone();
    // Wisdom 15 (+1 Human): three prepared spells at level 1
    let s = app.state.set_base_score(&id, "wis", 15).expect("wisdom");
    assert_eq!(app.play("set_base_score", &s)["concentration"], Value::Null);
    app.state
        .set_spell_prepared(&id, "main", BLESS, true)
        .expect("prepare Bless");
    let s = app
        .state
        .set_spell_prepared(&id, "main", CURE_WOUNDS, true)
        .expect("prepare Cure Wounds");
    let play = app.play("set_spell_prepared", &s);
    let names: Vec<&str> = play["concentrationSpells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Bless"]);

    let s = app
        .state
        .set_concentration(&id, Some(BLESS))
        .expect("concentrate");
    assert_eq!(
        app.play("set_concentration", &s)["concentration"],
        json!({"id": BLESS, "name": "Bless", "level": 1})
    );
    let err = app
        .state
        .set_concentration(&id, Some(CURE_WOUNDS))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);

    let s = app.state.long_rest(&id).expect("long rest");
    assert_eq!(app.play("long_rest", &s)["concentration"], Value::Null);
    app.state
        .set_concentration(&id, Some(BLESS))
        .expect("again");
    let s = app.state.set_concentration(&id, None).expect("end");
    assert_eq!(
        app.play("set_concentration", &s)["concentration"],
        Value::Null
    );
}

#[test]
fn custom_trackers_are_added_edited_tracked_deleted_and_saved() {
    let app = App::new();
    let s = app.create(
        Ruleset::Dnd2014,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_FIGHTER",
    );
    let id = s.id.clone();
    assert_eq!(app.play("get_snapshot", &s)["trackers"], json!([]));
    let s = app
        .state
        .add_tracker(&id, "Infection", 6, TrackerReset::Never)
        .expect("add");
    let s2 = app
        .state
        .add_tracker(&id, "Splicer Points", 5, TrackerReset::LongRest)
        .expect("add");
    let trackers = app.play("add_tracker", &s2)["trackers"].clone();
    assert_eq!(trackers.as_array().unwrap().len(), 2);
    let infection = app.play("add_tracker", &s)["trackers"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let splicer = trackers[1]["id"].as_str().unwrap().to_string();
    assert_eq!(
        trackers[1],
        json!({"id": splicer, "name": "Splicer Points", "max": 5, "used": 0, "reset": "long_rest"})
    );

    app.state
        .set_tracker_used(&id, &infection, 2)
        .expect("track");
    let s = app.state.set_tracker_used(&id, &splicer, 5).expect("track");
    app.play("set_tracker_used", &s);
    assert_eq!(
        app.state
            .set_tracker_used(&id, &splicer, 6)
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        app.state
            .set_tracker_used(&id, "tracker-99", 0)
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    assert_eq!(
        app.state
            .add_tracker(&id, " ", 3, TrackerReset::Never)
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    let s = app.state.long_rest(&id).expect("long rest");
    let used: Vec<u64> = app.play("long_rest", &s)["trackers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["used"].as_u64().unwrap())
        .collect();
    assert_eq!(used, vec![2, 0], "the infection never resets");

    let s = app
        .state
        .update_tracker(&id, &infection, "Contamination", 1, TrackerReset::ShortRest)
        .expect("edit");
    assert_eq!(
        app.play("update_tracker", &s)["trackers"][0],
        json!({"id": infection, "name": "Contamination", "max": 1, "used": 1, "reset": "short_rest"})
    );
    let s = app.state.remove_tracker(&id, &splicer).expect("delete");
    assert_eq!(
        app.play("remove_tracker", &s)["trackers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // saved with the character and back after closing and loading it
    app.state.save_character(&id).expect("save");
    app.state.close_character(&id, false).expect("close");
    let s = app.state.load_character(&id).expect("load");
    assert_eq!(
        app.play("load_character", &s)["trackers"],
        json!([{"id": infection, "name": "Contamination", "max": 1, "used": 1, "reset": "short_rest"}])
    );
}

#[test]
fn a_saved_character_that_tracks_nothing_loads_with_nothing_tracked() {
    let app = App::new();
    let s = app.wizard(1);
    let id = s.id.clone();
    app.state.save_character(&id).expect("save");
    let path = app.state.character_path(&id).expect("path");
    let text = std::fs::read_to_string(path).expect("file");
    for key in [
        "spell_slots_used",
        "pact_slots_used",
        "concentration",
        "trackers",
    ] {
        assert!(
            !text.contains(key),
            "{key} written for an untracked character"
        );
    }
    app.state.close_character(&id, true).expect("close");
    let s = app.state.load_character(&id).expect("load");
    let play = app.play("load_character", &s);
    assert_eq!(slot_rows(&play), vec![(1, 2, 0)]);
    assert_eq!(play["trackers"], json!([]));
    assert_eq!(play["concentration"], Value::Null);
}
