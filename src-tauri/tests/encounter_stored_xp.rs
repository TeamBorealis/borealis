//! Decision HB5 through the command layer: `encounter_evaluate` rates a homebrew monster at the XP
//! its stat block prints, not the table XP of its challenge rating. Official monsters are unchanged.

use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// SYNTHETIC homebrew: a CR 1/4 creature (table 50 XP) whose stat block prints 75 XP.
const HOMEBREW: &str = "ID_TEST_MONSTER_BRINDLEWICK_MOSSGNAWER";
const OFFICIAL: &str = "ID_SRD521_MONSTER_GOBLIN_WARRIOR";

fn content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

/// Write a user-library pack holding the synthetic monster (cloned from the Goblin Warrior).
fn install_homebrew(library: &Path) {
    let text =
        std::fs::read_to_string(content_root().join("srd-5.2.1-monsters/monsters.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let mut monster = file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == OFFICIAL)
        .expect("goblin warrior")
        .clone();
    let obj = monster.as_object_mut().unwrap();
    obj.remove("maps_to");
    obj.remove("renamed_from");
    obj.insert("id".into(), json!(HOMEBREW));
    obj.insert("name".into(), json!("Brindlewick Mossgnawer"));
    obj.insert("source".into(), json!("Test Homebrew"));
    monster["data"]["xp"] = json!(75);

    let dir = library.join("test-homebrew");
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
}

fn evaluate(state: &AppState, id: &str) -> Value {
    state
        .encounter_evaluate(&json!({
            "partyLevels": [1, 1, 1, 1],
            "ruleset": "dnd2024",
            "monsters": [{"id": id, "count": 3}]
        }))
        .unwrap()
}

#[test]
fn encounter_evaluate_uses_the_printed_xp_of_homebrew_and_the_table_for_official() {
    // Arrange
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: content_root(),
        data: data.path().to_path_buf(),
    };
    install_homebrew(&paths.library());
    let state = AppState::new(paths).unwrap();

    // Act
    let homebrew = evaluate(&state, HOMEBREW);
    let official = evaluate(&state, OFFICIAL);

    // Assert
    assert_eq!(homebrew["monsters"][0]["xpEach"], 75, "{homebrew}");
    assert_eq!(homebrew["report"]["totalXp"], 225, "{homebrew}");
    assert_eq!(official["monsters"][0]["xpEach"], 50, "{official}");
    assert_eq!(official["report"]["totalXp"], 150, "{official}");
}
