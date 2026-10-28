//! B-55 W3 (`specs/version-rows-and-filters.md` 5): the equipment add-item list offers what the
//! engine would let the character use, so its Edition filter has something to filter: with the
//! other edition allowed (strict mode off) other-edition items are listed, superseded ones never;
//! `bundled` marks SRD items for the default version order; a category counts one row per name.
//! Bundled SRD packs plus a synthetic imported pack (no corpus, no 5etools text).

mod contract_support;

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::ItemFilter;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const GLAIVE14: &str = "ID_T_W14_GLAIVE";
const LANTERN14: &str = "ID_T_GEAR14_LANTERN";
const LANTERN24: &str = "ID_T_GEAR24_LANTERN";
const LONGSWORD14: &str = "ID_WOTC_PHB_WEAPON_LONGSWORD";
const LONGSWORD24: &str = "ID_2024_WEAPON_LONGSWORD";
const GEAR: &str = "Adventuring Gear";
const WEAPONS: &str = "Weapons";

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    std::fs::write(path, serde_json::to_string_pretty(value).expect("json")).expect("write");
}

fn gear(id: &str, name: &str, source: &str) -> Value {
    json!({"id": id, "name": name, "type": "Item", "source": source,
        "setters": [{"name": "category", "value": GEAR}, {"name": "cost", "value": "5", "attributes": {"currency": "gp"}}]})
}

/// Writes the synthetic pack: a 2014 glaive and lantern, a 2024 lantern of another source.
fn install(data: &Path) {
    let dir = data.join("library/b55-items");
    let file = |ruleset: &str, elements: Vec<Value>| json!({"format": "borealis-content/1", "ruleset": ruleset, "elements": elements});
    let glaive = json!({"id": GLAIVE14, "name": "Test Glaive", "type": "Weapon", "source": "Test Imports",
        "setters": [{"name": "category", "value": WEAPONS}, {"name": "damage", "value": "1d10", "attributes": {"type": "slashing"}}]});
    write_json(
        &dir.join("legacy.json"),
        &file(
            "dnd2014",
            vec![glaive, gear(LANTERN14, "Test Lantern", "Test Imports")],
        ),
    );
    write_json(
        &dir.join("modern.json"),
        &file(
            "dnd2024",
            vec![gear(LANTERN24, "Test  lantern", "Test Imports Modern")],
        ),
    );
    write_json(
        &dir.join("pack.json"),
        &json!({"format": "borealis-pack/1", "id": "b55-items", "name": "B-55 item imports",
            "version": "1.0.0", "origin": "custom", "rulesets": ["dnd2014", "dnd2024"],
            "files": [{"path": "legacy.json", "kind": "content"}, {"path": "modern.json", "kind": "content"}]}),
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

fn character(app: &AppState, ruleset: Ruleset) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Ilse".into(),
        ruleset,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    app.create_character(ruleset, &request).expect("create")
}

/// Every row of a category, as the UI receives it.
fn listing(app: &AppState, id: &str, category: &str) -> Value {
    let page = app
        .search_items(&ItemFilter {
            category: Some(category.into()),
            query: None,
            source: None,
            limit: Some(1000),
            offset: None,
            character_id: Some(id.into()),
        })
        .expect("search");
    serde_json::to_value(page).expect("json")
}

fn ids(page: &Value) -> BTreeSet<String> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| i["id"].as_str().expect("id").to_string())
        .collect()
}

fn row<'v>(page: &'v Value, id: &str) -> &'v Value {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["id"] == id)
        .unwrap_or_else(|| panic!("no {id}"))
}

#[test]
fn a_2024_character_lists_other_edition_items_while_the_other_edition_is_allowed() {
    let (_d, app) = app();
    let snap = character(&app, Ruleset::Dnd2024);
    let gear = ids(&listing(&app, &snap.id, GEAR));
    assert!(
        gear.contains(LANTERN24) && gear.contains(LANTERN14),
        "{gear:?}"
    );
    assert!(ids(&listing(&app, &snap.id, WEAPONS)).contains(GLAIVE14));
    let strict = app
        .set_allow_cross_edition(&snap.id, Some(false))
        .expect("strict on");
    let gear = ids(&listing(&app, &strict.id, GEAR));
    assert!(
        gear.contains(LANTERN24) && !gear.contains(LANTERN14),
        "{gear:?}"
    );
    assert!(!ids(&listing(&app, &strict.id, WEAPONS)).contains(GLAIVE14));
}

#[test]
fn a_2014_character_lists_2024_items_only_with_strict_mode_off() {
    let (_d, app) = app();
    let snap = character(&app, Ruleset::Dnd2014);
    let gear = ids(&listing(&app, &snap.id, GEAR));
    assert!(
        gear.contains(LANTERN14) && !gear.contains(LANTERN24),
        "{gear:?}"
    );
    let open = app
        .set_allow_cross_edition(&snap.id, Some(true))
        .expect("strict off");
    let gear = ids(&listing(&app, &open.id, GEAR));
    assert!(
        gear.contains(LANTERN14) && gear.contains(LANTERN24),
        "{gear:?}"
    );
}

#[test]
fn a_superseded_srd_item_is_not_listed() {
    let (_d, app) = app();
    let snap = character(&app, Ruleset::Dnd2024);
    let weapons = ids(&listing(&app, &snap.id, WEAPONS));
    assert!(weapons.contains(LONGSWORD24));
    assert!(
        !weapons.contains(LONGSWORD14),
        "the 5.2.1 Longsword replaces the 5.1 one"
    );
}

#[test]
fn bundled_marks_srd_items_and_the_page_matches_the_contract() {
    let (_d, app) = app();
    let snap = character(&app, Ruleset::Dnd2024);
    let weapons = listing(&app, &snap.id, WEAPONS);
    assert_eq!(row(&weapons, LONGSWORD24)["bundled"], true);
    assert!(
        row(&weapons, GLAIVE14).get("bundled").is_none(),
        "imported is not bundled"
    );
    assert_eq!(row(&weapons, GLAIVE14)["rulesets"], json!(["dnd2014"]));
    let errs = Contract::load().response("search_items", &weapons);
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn a_category_counts_one_row_per_name() {
    let (_d, app) = app();
    let snap = character(&app, Ruleset::Dnd2024);
    let page = listing(&app, &snap.id, GEAR);
    let names: BTreeSet<String> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| {
            let name = i["name"].as_str().expect("name");
            let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("{}|{}", i["type"], name.to_lowercase())
        })
        .collect();
    assert!(
        names.len() < page["items"].as_array().unwrap().len(),
        "the two lanterns share a row"
    );
    let cats = app.list_item_categories(&snap.id).expect("categories");
    let count = cats.iter().find(|c| c.name == GEAR).expect("gear").count;
    assert_eq!(count, names.len());
}
