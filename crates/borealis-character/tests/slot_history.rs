//! The per-slot replacement history is backward compatible.

use borealis_character::Character;
use borealis_engine::state::{ReplacedPick, SlotHistory, SlotKey};

#[test]
fn files_without_slot_history_still_load() {
    let text = r#"{"format":"borealis-character","format_version":2,"ruleset":"dnd2014","build":{
        "ruleset":"dnd2014","levels":[{"kind":"main"}],
        "selections":[{"slot":{"owner":"ID_X","rule":3,"ordinal":1},"choice":{"element":"ID_S"}}]}}"#;
    let c = Character::from_json_str(text).expect("old file loads");
    assert!(c.build.slot_history.is_empty());
    assert_eq!(c.build.selections.len(), 1);
    let out = c.to_json_string().unwrap();
    assert!(
        !out.contains("slot_history"),
        "empty history is not written"
    );
}

#[test]
fn slot_history_round_trips() {
    let mut c = Character::new("Mira");
    c.build.slot_history.push(SlotHistory {
        slot: SlotKey::new("ID_X", 3, 1),
        replaced: vec![ReplacedPick {
            id: "ID_S".into(),
            level: 5,
        }],
    });
    let text = c.to_json_string().unwrap();
    assert!(text.contains("slot_history"));
    let back = Character::from_json_str(&text).unwrap();
    assert_eq!(
        back.build.replaced_in(&SlotKey::new("ID_X", 3, 1))[0].level,
        5
    );
    assert_eq!(back, c);
}
