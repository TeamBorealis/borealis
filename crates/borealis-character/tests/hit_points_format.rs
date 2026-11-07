//! Hit points and hit dice on Play (B-194) are saved with the character in Borealis's own format
//! (`build.play.hp_lost`, `temp_hp`, `hit_dice_used`): a file written before them loads at full
//! hit points with every hit die, the values survive a save and load, nothing is written at full
//! hit points, and a broken value is repaired instead of stopping the character. An app older
//! than these keys keeps them through its save (the unknown-key mechanism of 1.2.1,
//! `play_tracker_format.rs`).

use borealis_character::Character;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// A 1.3.0-era character file: play state with a resource use and a tracker, no hit points.
const OLD_FILE: &str = r##"{"format":"borealis-character","format_version":2,"ruleset":"dnd2014","build":{
  "ruleset":"dnd2014","levels":[{"kind":"main"}],
  "play":{"resources_used":{"second-wind":1},"trackers":[{"id":"tracker-1","name":"Luck","max":3,"used":1,"reset":"never"}]}}}"##;

fn file_with_play(play: &Value) -> String {
    json!({"format": "borealis-character", "format_version": 2, "ruleset": "dnd2014",
        "build": {"ruleset": "dnd2014", "levels": [{"kind": "main"}], "play": play}})
    .to_string()
}

#[test]
fn a_file_from_before_hit_points_loads_at_full_hit_points_with_every_hit_die() {
    let c = Character::from_json_str(OLD_FILE).expect("old file loads");
    let p = &c.build.play;
    assert_eq!((p.hp_lost, p.temp_hp), (0, 0));
    assert!(p.hit_dice_used.is_empty());
    assert_eq!(p.resources_used["second-wind"], 1);
    let text = c.to_json_string().expect("save");
    for key in ["hp_lost", "temp_hp", "hit_dice_used"] {
        assert!(
            !text.contains(key),
            "{key} is not written at full hit points"
        );
    }
}

#[test]
fn hit_points_and_spent_hit_dice_survive_save_and_load() {
    let mut c = Character::from_json_str(OLD_FILE).expect("old file loads");
    c.build.play.hp_lost = 7;
    c.build.play.temp_hp = 5;
    c.build.play.hit_dice_used = BTreeMap::from([
        ("ID_WOTC_PHB_CLASS_FIGHTER".to_string(), 2),
        ("ID_WOTC_PHB_MULTICLASS_WIZARD".to_string(), 1),
    ]);
    let text = c.to_json_string().expect("save");
    let saved: Value = serde_json::from_str(&text).expect("json");
    let play = &saved["build"]["play"];
    assert_eq!(play["hp_lost"], json!(7));
    assert_eq!(play["temp_hp"], json!(5));
    assert_eq!(
        play["hit_dice_used"],
        json!({"ID_WOTC_PHB_CLASS_FIGHTER": 2, "ID_WOTC_PHB_MULTICLASS_WIZARD": 1})
    );
    assert!(
        play.get("extra").is_none(),
        "known keys are not repeated as extras"
    );
    let back = Character::from_json_str(&text).expect("reload");
    assert_eq!(back, c);
    assert!(back.build.play.extra.is_empty());
}

#[test]
fn broken_hit_point_values_are_repaired() {
    let cases = [
        json!({"hp_lost": -4, "temp_hp": "lots"}),
        json!({"hit_dice_used": {"ID_A": 2, "ID_B": -1, "ID_C": "x", "ID_D": 0, "": 3}}),
        json!({"hit_dice_used": [1, 2]}),
    ];
    for play in &cases {
        let c = Character::from_json_str(&file_with_play(play))
            .unwrap_or_else(|e| panic!("{play}: {e}"));
        let again = Character::from_json_str(&c.to_json_string().expect("save")).expect("reload");
        assert_eq!(again.build.play, c.build.play, "{play}");
        assert!(c.build.play.extra.is_empty(), "{play}");
    }
    let lost = Character::from_json_str(&file_with_play(&cases[0])).expect("load");
    assert_eq!((lost.build.play.hp_lost, lost.build.play.temp_hp), (0, 0));
    let dice = Character::from_json_str(&file_with_play(&cases[1])).expect("load");
    assert_eq!(
        dice.build.play.hit_dice_used,
        BTreeMap::from([("ID_A".to_string(), 2)])
    );
}
