//! Decision HB5: `encounter_evaluate` rates a monster given by id at its stored XP (the printed
//! value for homebrew). A bare `cr` has no stored XP and uses the table.

mod common;

use std::path::PathBuf;

use common::{content_root, Session};
use serde_json::{json, Value};

/// SYNTHETIC homebrew: a CR 1/4 creature (table 50 XP) whose stat block prints 75 XP.
const HOMEBREW: &str = "ID_TEST_MONSTER_BRINDLEWICK_MOSSGNAWER";

/// A user library holding one pack with the synthetic monster (cloned from the Goblin Warrior).
fn library() -> PathBuf {
    let text =
        std::fs::read_to_string(content_root().join("srd-5.2.1-monsters/monsters.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let mut monster = file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Goblin Warrior")
        .expect("goblin warrior")
        .clone();
    assert_eq!(monster["data"]["cr"], "1/4");
    let obj = monster.as_object_mut().unwrap();
    obj.remove("maps_to");
    obj.remove("renamed_from");
    obj.insert("id".into(), json!(HOMEBREW));
    obj.insert("name".into(), json!("Brindlewick Mossgnawer"));
    obj.insert("source".into(), json!("Test Homebrew"));
    monster["data"]["xp"] = json!(75);

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("hb5-library-{}", std::process::id()))
        .join("test-homebrew");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "format": "borealis-pack/1",
        "id": "test-homebrew",
        "name": "Test Homebrew",
        "version": "1.0.0",
        "origin": "custom",
        "rulesets": ["dnd2024"],
        "files": [{"path": "monsters.json", "kind": "content"}]
    });
    let content =
        json!({"format": "borealis-content/1", "ruleset": "dnd2024", "elements": [monster]});
    std::fs::write(dir.join("pack.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("monsters.json"), content.to_string()).unwrap();
    dir.parent().unwrap().to_path_buf()
}

fn evaluate(s: &mut Session, monsters: Value) -> Value {
    let r = s.call(
        "encounter_evaluate",
        json!({"party_levels": [1, 1, 1, 1], "monsters": monsters, "ruleset": "2024"}),
    );
    assert_eq!(r["isError"], false, "{r}");
    r["structuredContent"].clone()
}

#[test]
fn a_homebrew_monster_by_id_counts_its_printed_xp() {
    let lib = library();
    let mut s = Session::start_with_library(Some(&lib));

    let out = evaluate(&mut s, json!([{"id": HOMEBREW, "count": 3}]));

    assert_eq!(out["total_xp"], 225, "{out}");
    assert_eq!(out["band"], "Moderate", "{out}");
}

#[test]
fn official_monsters_and_bare_crs_keep_the_table_xp() {
    let mut s = Session::start();
    let by_id = evaluate(
        &mut s,
        json!([{"id": "ID_SRD521_MONSTER_GOBLIN_WARRIOR", "count": 3}]),
    );
    let by_cr = evaluate(&mut s, json!([{"cr": "1/4", "count": 3}]));
    assert_eq!(by_id["total_xp"], 150);
    assert_eq!(by_cr["total_xp"], 150);
}
