//! The Play tab spell tracker (GitHub #2) is saved with the character in Borealis's own format:
//! a file written before it loads with nothing tracked, and the tracked state survives a save and
//! load. Nothing is written for a character that tracks nothing.

use borealis_character::Character;
use borealis_engine::state::{CustomTracker, TrackerReset};

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
        },
        CustomTracker {
            id: "tracker-2".into(),
            name: "Splicer Points".into(),
            max: 5,
            used: 0,
            reset: TrackerReset::LongRest,
        },
    ];
    let text = c.to_json_string().expect("save");
    assert!(text.contains("\"reset\": \"long_rest\""), "{text}");
    let back = Character::from_json_str(&text).expect("reload");
    assert_eq!(back, c);
}
