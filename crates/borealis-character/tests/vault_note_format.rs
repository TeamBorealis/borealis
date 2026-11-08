//! B-222: a character's linked vault note is kept in the character file as the top-level `vault_note`, a
//! vault-relative path. It is written only when set, so a file without a link saves as before.

use borealis_character::Character;
use serde_json::Value;

/// A 1.3.0-era character file: no vault note.
const OLD_FILE: &str = r#"{"format":"borealis-character","format_version":2,"ruleset":"dnd2014",
  "meta":{"name":"Brann","group":"Characters"},"build":{"ruleset":"dnd2014","levels":[{"kind":"main"}]}}"#;

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("json")
}

#[test]
fn a_file_without_a_note_loads_unlinked_and_saves_without_the_key() {
    let c = Character::from_json_str(OLD_FILE).expect("old file loads");
    assert_eq!(c.vault_note, None);
    let text = c.to_json_string().expect("save");
    assert!(json(&text).get("vault_note").is_none(), "{text}");
    assert!(c.extra.is_empty());
}

#[test]
fn the_linked_note_survives_save_and_load() {
    let mut c = Character::from_json_str(OLD_FILE).expect("old file loads");
    c.vault_note = Some("Characters/Brann.md".into());
    let text = c.to_json_string().expect("save");
    assert_eq!(json(&text)["vault_note"], "Characters/Brann.md");
    let back = Character::from_json_str(&text).expect("reload");
    assert_eq!(back, c);
    assert!(
        back.extra.is_empty(),
        "the link is a known key, not an unknown one"
    );
    c.vault_note = None;
    let cleared = c.to_json_string().expect("save");
    assert!(json(&cleared).get("vault_note").is_none(), "{cleared}");
}
