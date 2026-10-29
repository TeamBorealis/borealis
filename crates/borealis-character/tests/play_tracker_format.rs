//! The Play tab spell tracker (GitHub #2) is saved with the character in Borealis's own format:
//! a file written before it loads with nothing tracked, and the tracked state survives a save and
//! load. Nothing is written for a character that tracks nothing.

use borealis_character::Character;
use borealis_engine::state::{CustomTracker, TrackerReset};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// A 1.2.0-era character file: play state with resource uses only.
const OLD_FILE: &str = r##"{"format":"borealis-character","format_version":2,"ruleset":"dnd2024","build":{
  "ruleset":"dnd2024","levels":[{"kind":"main"}],
  "play":{"resources_used":{"rage":1},"toggles":[],"exhaustion":0,"heroic_inspiration":false}}}"##;

#[test]
fn a_file_from_before_the_tracker_loads_with_nothing_tracked() {
    let c = Character::from_json_str(OLD_FILE).expect("old file loads");
    let p = &c.build.play;
    assert_eq!(p.resources_used["rage"], 1);
    assert!(p.spell_slots_used.is_empty());
    assert_eq!(p.pact_slots_used, 0);
    assert_eq!(p.concentration, None);
    assert!(p.trackers.is_empty());
    let text = c.to_json_string().expect("save");
    for key in [
        "spell_slots_used",
        "pact_slots_used",
        "concentration",
        "trackers",
    ] {
        assert!(
            !text.contains(key),
            "{key} is not written when nothing is tracked"
        );
    }
}

#[test]
fn tracked_slots_concentration_and_trackers_survive_save_and_load() {
    let mut c = Character::from_json_str(OLD_FILE).expect("old file loads");
    let p = &mut c.build.play;
    p.spell_slots_used.insert(1, 2);
    p.spell_slots_used.insert(3, 1);
    p.pact_slots_used = 1;
    p.concentration = Some("ID_PHB_SPELL_BLESS".into());
    p.trackers = vec![
        CustomTracker {
            id: "tracker-1".into(),
            name: "Infection".into(),
            max: 6,
            used: 2,
            reset: TrackerReset::Never,
            counts_up: true,
            extra: BTreeMap::new(),
        },
        CustomTracker {
            id: "tracker-2".into(),
            name: "Splicer Points".into(),
            max: 5,
            used: 0,
            reset: TrackerReset::LongRest,
            counts_up: false,
            extra: BTreeMap::new(),
        },
    ];
    let text = c.to_json_string().expect("save");
    assert!(text.contains("\"reset\": \"long_rest\""), "{text}");
    let back = Character::from_json_str(&text).expect("reload");
    assert_eq!(back, c);
}

/// Saves written by the app before the tracker existed (18859de, `Character::to_json_string`):
/// loading and saving them now gives the same bytes.
#[test]
fn a_character_that_tracks_nothing_saves_byte_for_byte_as_before() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/prefeature-1.2");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("fixtures") {
        let path = entry.expect("entry").path();
        let before = std::fs::read_to_string(&path).expect("read");
        let c = Character::from_json_str(&before).expect("load");
        assert_eq!(
            c.to_json_string().expect("save"),
            before,
            "{}",
            path.display()
        );
        seen += 1;
    }
    assert_eq!(seen, 3);
}

fn file_with_play(play: &Value) -> String {
    json!({"format": "borealis-character", "format_version": 2, "ruleset": "dnd2024",
        "build": {"ruleset": "dnd2024", "levels": [{"kind": "main"}], "play": play}})
    .to_string()
}

#[test]
fn a_hostile_play_state_loads_repaired_and_never_stops_the_character() {
    let mut many = Vec::new();
    for i in 0..10_000 {
        many.push(json!({"id": format!("t{i}"), "name": "n", "max": 1}));
    }
    let cases = [
        json!({"spell_slots_used": {"0": 1, "42": 2, "x": 3, "2": -1, "3": 2, "4": 0}}),
        json!({"pact_slots_used": -3, "exhaustion": "very", "toggles": 7}),
        json!({"concentration": 12, "resources_used": {"rage": "lots"}}),
        json!({"concentration": "ID_NO_SUCH_SPELL"}),
        json!({"trackers": [
            {"id": "tracker-1", "name": "A", "max": 3, "used": 9},
            {"id": "tracker-1", "name": "B", "max": 0},
            {"id": "", "name": "\u{202e}C\u{200b}", "max": 5000, "reset": "dawn"},
            {"name": "no id"},
            {"id": "tracker-9", "max": 2},
            "not a tracker",
            {"id": "tracker-2", "name": "", "max": 1, "used": -1}
        ]}),
        json!({"trackers": many}),
        json!({"trackers": {"not": "a list"}}),
        json!("not a map"),
    ];
    for play in &cases {
        let c = Character::from_json_str(&file_with_play(play))
            .unwrap_or_else(|e| panic!("{play}: {e}"));
        let p = &c.build.play;
        assert!(p.spell_slots_used.keys().all(|l| (1..=9).contains(l)));
        assert!(p.trackers.len() <= 50);
        let mut ids: Vec<&str> = p.trackers.iter().map(|t| t.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), p.trackers.len(), "unique ids");
        for t in &p.trackers {
            assert!((1..=999).contains(&t.max) && t.used <= t.max, "{t:?}");
            assert!(!t.name.is_empty() && !t.name.contains('\u{202e}'));
        }
        // and it saves and loads again
        let again = Character::from_json_str(&c.to_json_string().expect("save")).expect("reload");
        assert_eq!(again.build.play, c.build.play);
    }
    let slots = Character::from_json_str(&file_with_play(&cases[0])).expect("load");
    assert_eq!(slots.build.play.spell_slots_used, BTreeMap::from([(3, 2)]));
    let ts = Character::from_json_str(&file_with_play(&cases[4])).expect("load");
    let t = &ts.build.play.trackers;
    let summary: Vec<(&str, u32, u32)> =
        t.iter().map(|t| (t.name.as_str(), t.max, t.used)).collect();
    assert_eq!(
        summary,
        vec![("A", 3, 3), ("B", 1, 0), ("C", 999, 0), ("Tracker", 2, 0)],
        "entries without a max or with a negative count dropped; the rest repaired (new ids, fallback name)"
    );
    assert_eq!(
        t[2].reset,
        TrackerReset::Other("dawn".into()),
        "a newer reset is kept"
    );
    let text = ts.to_json_string().expect("save");
    assert!(text.contains("\"reset\": \"dawn\""));
}

#[test]
fn keys_from_a_newer_version_survive_a_save() {
    let text = json!({"format": "borealis-character", "format_version": 2, "ruleset": "dnd2024",
        "build": {"ruleset": "dnd2024", "levels": [{"kind": "main"}], "future_build_key": {"a": 1},
            "play": {"exhaustion": 1, "future_play_key": [1, 2],
                "trackers": [{"id": "tracker-1", "name": "A", "max": 2, "future_tracker_key": true}]}}})
    .to_string();
    let c = Character::from_json_str(&text).expect("load");
    let saved: Value = serde_json::from_str(&c.to_json_string().expect("save")).expect("json");
    assert_eq!(saved["build"]["future_build_key"], json!({"a": 1}));
    assert_eq!(saved["build"]["play"]["future_play_key"], json!([1, 2]));
    assert_eq!(
        saved["build"]["play"]["trackers"][0]["future_tracker_key"],
        json!(true)
    );
}
