//! Play tab hit points and hit dice (B-194, decision PL1) through the real command layer: damage,
//! healing, temporary hit points and hit dice change the snapshot's `play.hitPoints` and
//! `play.hitDice`, the existing rests recover them, everything is saved with the character, and
//! every response matches the TS contract.

mod contract_support;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::Value;
use std::path::PathBuf;

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

    /// The snapshot's `play` as the UI receives it, after checking it against the TS type.
    fn play(&self, command: &str, snap: &CharacterSnapshot) -> Value {
        let v = serde_json::to_value(snap).expect("json");
        let errors = self.contract.response(command, &v);
        assert!(errors.is_empty(), "{command}: {errors:?}");
        v["play"].clone()
    }

    fn fighter(&self, ruleset: Ruleset, race: &str, class: &str, level: u32) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Hit Points".into(),
            ruleset,
            starting_level: 1,
            gender: String::new(),
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
        let mut s = self
            .state
            .apply_selection(&s.id, &slot_id(&s, "Class"), class)
            .expect("class");
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

fn hp(play: &Value) -> (u64, u64, u64) {
    let h = &play["hitPoints"];
    (
        h["current"].as_u64().expect("current"),
        h["max"].as_u64().expect("max"),
        h["temp"].as_u64().expect("temp"),
    )
}

/// (classId, left, total) of the first class's hit dice.
fn dice(play: &Value) -> (String, u64, u64) {
    let d = &play["hitDice"][0];
    let total = d["total"].as_u64().expect("total");
    (
        d["classId"].as_str().expect("classId").to_string(),
        total - d["used"].as_u64().expect("used"),
        total,
    )
}

#[test]
fn damage_healing_temp_and_hit_dice_through_the_commands_and_the_rests() {
    // (edition, race, class, hit dice left after the long rest)
    for (ruleset, race, class, after_long) in [
        (
            Ruleset::Dnd2014,
            "ID_RACE_HUMAN",
            "ID_WOTC_PHB_CLASS_FIGHTER",
            3,
        ),
        (
            Ruleset::Dnd2024,
            "ID_2024_SPECIES_HUMAN",
            "ID_2024_CLASS_FIGHTER",
            4,
        ),
    ] {
        let app = App::new();
        let s = app.fighter(ruleset, race, class, 4);
        let id = s.id.clone();
        let play = app.play("load_character", &s);
        let (_, max, _) = hp(&play);
        assert_eq!(hp(&play), (max, max, 0), "{ruleset:?}: full at first");
        let (class_id, left, total) = dice(&play);
        assert_eq!((left, total), (4, 4));

        app.state.set_temp_hit_points(&id, 3).expect("temp");
        let s = app.state.take_damage(&id, 8).expect("damage");
        assert_eq!(hp(&app.play("take_damage", &s)), (max - 5, max, 0));
        let s = app.state.regain_hit_points(&id, 2).expect("heal");
        assert_eq!(hp(&app.play("regain_hit_points", &s)).0, max - 3);
        let roll = 1;
        let s = app
            .state
            .spend_hit_die(&id, &class_id, roll)
            .expect("spend");
        let play = app.play("spend_hit_die", &s);
        assert_eq!(dice(&play).1, 3);
        assert!(hp(&play).0 > max - 3, "a spent die heals");
        // three more: none left
        for _ in 0..3 {
            app.state.spend_hit_die(&id, &class_id, 1).expect("spend");
        }
        let refused = app.state.spend_hit_die(&id, &class_id, 1).unwrap_err();
        assert_eq!(refused.code, ErrorCode::RequirementFailed);
        assert_eq!(
            app.state.take_damage(&id, 0).unwrap_err().code,
            ErrorCode::InvalidArgument
        );
        let s = app
            .state
            .restore_hit_die(&id, &class_id, 1)
            .expect("restore");
        assert_eq!(dice(&app.play("restore_hit_die", &s)).1, 1);
        let bad = app.state.spend_hit_die(&id, &class_id, 99).unwrap_err();
        assert_eq!(bad.code, ErrorCode::InvalidArgument, "a d10 cannot roll 99");
        let s = app.state.take_damage(&id, 5).expect("damage");
        let hurt = hp(&app.play("take_damage", &s)).0;

        // a short rest changes no hit points; saved and loaded, then a long rest by the edition
        let s = app.state.short_rest(&id).expect("short rest");
        assert_eq!(hp(&app.play("short_rest", &s)).0, hurt);
        app.state.save_character(&id).expect("save");
        app.state.close_character(&id, false).expect("close");
        let s = app.state.load_character(&id).expect("load");
        let play = app.play("load_character", &s);
        assert_eq!(hp(&play).0, hurt, "{ruleset:?}: saved");
        assert_eq!(dice(&play).1, 1, "{ruleset:?}: saved");
        let s = app.state.long_rest(&id).expect("long rest");
        let play = app.play("long_rest", &s);
        assert_eq!(hp(&play), (max, max, 0));
        // 2014: half of four come back (one die left, three spent, two back); 2024: all
        assert_eq!(dice(&play).1, after_long, "{ruleset:?}");
    }
}
