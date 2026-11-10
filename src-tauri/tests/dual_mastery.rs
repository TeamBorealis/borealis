//! Weapons with more than one Mastery property (RL10, reported by RJBPrime). Through the app's
//! API: the weapon's card and facts, the add-item list, the inventory row, the Weapon Mastery
//! pick and the attack row each show every property, with its text where the UI shows text.
//! A character saved before (prefeature 1.2 fixture) derives the same single properties.
//! Bundled SRD packs plus a synthetic homebrew pack (invented names only).

use borealis_character::Character;
use borealis_lib::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, Ruleset};
use borealis_lib::dto_ext::{EquipRequest, ItemFilter};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const THORNGLAIVE: &str = "ID_HB_WEAPON_THORNGLAIVE";
const BRIARSTAFF: &str = "ID_HB_WEAPON_BRIARSTAFF";

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    std::fs::write(path, serde_json::to_string_pretty(value).expect("json")).expect("write");
}

/// Thornglaive names two properties by id; Briarstaff names two by `supports`, as Aurora-format
/// content does (no `data`).
fn install(data: &Path) {
    let dir = data.join("library/dual-mastery");
    let thornglaive = json!({
        "id": THORNGLAIVE, "name": "Thornglaive", "type": "Weapon", "source": "My Homebrew",
        "supports": ["ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE", "ID_INTERNAL_DAMAGE_TYPE_SLASHING"],
        "setters": [
            {"name": "category", "value": "Weapons"},
            {"name": "cost", "value": "25", "attributes": {"currency": "gp"}},
            {"name": "slot", "value": "onehand"},
            {"name": "damage", "value": "1d8", "attributes": {"type": "slashing"}},
            {"name": "mastery", "value": "Topple, Push"}
        ],
        "data": {"weapon_category": "Martial Melee", "melee": true,
            "mastery_id": "ID_2024_MASTERY_TOPPLE",
            "mastery_ids": ["ID_2024_MASTERY_TOPPLE", "ID_2024_MASTERY_PUSH"]}
    });
    let briarstaff = json!({
        "id": BRIARSTAFF, "name": "Briarstaff", "type": "Weapon", "source": "My Homebrew",
        "supports": ["ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE", "ID_INTERNAL_DAMAGE_TYPE_BLUDGEONING",
            "ID_WOTC_PHB24_WEAPON_PROPERTY_SAP", "ID_WOTC_PHB24_WEAPON_PROPERTY_SLOW"],
        "setters": [
            {"name": "category", "value": "Weapons"},
            {"name": "slot", "value": "onehand"},
            {"name": "damage", "value": "1d6", "attributes": {"type": "bludgeoning"}}
        ]
    });
    write_json(
        &dir.join("weapons.json"),
        &json!({"format": "borealis-content/1", "ruleset": "dnd2024", "elements": [thornglaive, briarstaff]}),
    );
    write_json(
        &dir.join("pack.json"),
        &json!({"format": "borealis-pack/1", "id": "dual-mastery", "name": "Dual mastery homebrew",
            "version": "1.0.0", "origin": "custom", "rulesets": ["dnd2024"],
            "files": [{"path": "weapons.json", "kind": "content"}]}),
    );
}

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    install(dir.path());
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

fn fighter(app: &AppState) -> String {
    let request = NewCharacterRequest {
        name: "Wren".into(),
        ruleset: Ruleset::Dnd2024,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let snap = app
        .create_character(Ruleset::Dnd2024, &request)
        .expect("create");
    let class = snap
        .slots
        .iter()
        .find(|s| s.title == "Class")
        .expect("class slot")
        .id
        .clone();
    app.apply_selection(&snap.id, &class, "ID_2024_CLASS_FIGHTER")
        .expect("class");
    snap.id
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .expect("list")
        .iter()
        .map(|m| m["name"].as_str().expect("name").to_string())
        .collect()
}

#[test]
fn the_card_and_facts_name_and_explain_every_property() {
    let (_d, app) = app();
    let glaive = app.get_element(THORNGLAIVE).expect("Thornglaive");
    let facts: Vec<&str> = glaive
        .facts
        .iter()
        .filter(|f| f.label == "Mastery")
        .map(|f| f.value.as_str())
        .collect();
    assert_eq!(facts, ["Topple, Push"]);
    let html = &glaive.description_html;
    let topple = html.find("Mastery: Topple").expect("Topple heading");
    let push = html.find("Mastery: Push").expect("Push heading");
    assert!(topple < push, "{html}");
    assert!(html.contains("Prone") && html.contains("10 feet"), "{html}");
    let staff = app.get_element(BRIARSTAFF).expect("Briarstaff");
    assert!(
        staff.description_html.contains("Mastery: Sap")
            && staff.description_html.contains("Mastery: Slow"),
        "{}",
        staff.description_html
    );
}

#[test]
fn the_pick_the_inventory_and_the_attack_list_every_property() {
    let (_d, app) = app();
    let id = fighter(&app);
    let page = app
        .search_items(&ItemFilter {
            category: Some("Weapons".into()),
            query: Some("Thornglaive".into()),
            limit: Some(10),
            character_id: Some(id.clone()),
            ..ItemFilter::default()
        })
        .expect("search");
    let row = serde_json::to_value(&page).expect("json")["items"][0].clone();
    assert_eq!(row["weapon"]["mastery"], "Topple, Push", "{row}");

    let snap = app
        .choose_weapon_masteries(&id, &[THORNGLAIVE.to_string(), BRIARSTAFF.to_string()])
        .expect("master both");
    let snap = serde_json::to_value(&snap).expect("json");
    let picks = snap["play"]["masteries"].as_array().expect("picks");
    let glaive = picks
        .iter()
        .find(|p| p["weapon"] == THORNGLAIVE)
        .expect("pick");
    assert_eq!(names(&glaive["properties"]), ["Topple", "Push"]);
    assert!(glaive["properties"][1]["text"]
        .as_str()
        .is_some_and(|t| t.contains("10 feet")));
    let staff = picks
        .iter()
        .find(|p| p["weapon"] == BRIARSTAFF)
        .expect("pick");
    assert_eq!(names(&staff["properties"]), ["Sap", "Slow"]);

    app.add_item(&id, THORNGLAIVE, 1, false, None).expect("add");
    let snap = app.get_snapshot(&id).expect("snapshot");
    let uid = snap
        .inventory
        .items
        .iter()
        .find(|i| i.item_id == THORNGLAIVE)
        .expect("line")
        .id
        .clone();
    let snap = app
        .equip_item(&id, &uid, EquipRequest::Primary)
        .expect("equip");
    let snap = serde_json::to_value(&snap).expect("json");
    let line = snap["inventory"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["itemId"] == THORNGLAIVE)
        .expect("line")
        .clone();
    assert_eq!(line["weapon"]["mastery"], "Topple, Push");
    let attack = snap["attacks"]
        .as_array()
        .expect("attacks")
        .iter()
        .find(|a| a["name"] == "Thornglaive")
        .expect("attack")
        .clone();
    assert_eq!(names(&attack["masteries"]), ["Topple", "Push"]);
    assert!(attack["masteries"]
        .as_array()
        .expect("list")
        .iter()
        .all(|m| m["mastered"] == true && !m["text"].as_str().unwrap_or("").is_empty()));
}

#[test]
fn a_character_saved_before_derives_the_same_single_properties() {
    let (_d, app) = app();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../crates/borealis-character/tests/data/prefeature-1.2/seraphine-vale-final-equipped.json",
    );
    let text = std::fs::read_to_string(path).expect("fixture");
    let c = Character::from_json_str(&text).expect("load");
    let hub = app.hub().expect("hub");
    let snap = hub.engine.derive(&c.build);
    let picks: Vec<(&str, Vec<&str>)> = snap
        .masteries
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.properties.iter().map(|m| m.name.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        picks,
        [
            ("Longsword", vec!["Sap"]),
            ("Dagger", vec!["Nick"]),
            ("Greatsword", vec!["Graze"])
        ]
    );
    let rows: Vec<(&str, Vec<(&str, bool)>)> = snap
        .attacks
        .iter()
        .filter(|a| !a.masteries.is_empty())
        .map(|a| {
            (
                a.name.as_str(),
                a.masteries
                    .iter()
                    .map(|m| (m.name.as_str(), m.mastered))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Dagger", vec![("Nick", true)]),
            ("Longsword", vec![("Sap", true)])
        ]
    );
}
