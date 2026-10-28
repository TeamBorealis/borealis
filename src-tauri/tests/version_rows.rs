//! B-55 (`specs/version-rows-and-filters.md` 1.1-1.2, 1.6, 3, 6): picker rows are one per
//! name, the versions of a row are the engine-offered options of that name, `total` counts
//! rows, a call without `limit` lists every row, and `switch_version` swaps a chosen row's
//! version keeping the sub-choices that still fit. Bundled SRD packs plus a synthetic imported
//! pack written into the app data folder (no corpus, no 5etools text).

mod contract_support;

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const SRD24: &str = "ID_2024_SPECIES_DRAGONBORN";
const SRD14: &str = "ID_RACE_DRAGONBORN";
const IMPORTED: &str = "ID_T_SPECIES_DRAGONBORN";
const TWIN: &str = "ID_T_SPECIES_DRAGONBORN_TWIN";
const RED24: &str = "ID_2024_OPTION_DRACONIC_ANCESTRY_RED";
const RED14: &str = "ID_RACIAL_TRAIT_DRACONIC_ANCESTRY_RED";
const MANY: &str = "ID_T_SPECIES_MANY";
const MANY_ROWS: usize = 250;

/// The synthetic imported pack: (2024 elements, 2014 elements).
struct Pack(Vec<Value>, Vec<Value>);

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    std::fs::write(path, serde_json::to_string_pretty(value).expect("json")).expect("write");
}

fn install(data: &Path, pack: &Pack) {
    let dir = data.join("library/b55-test");
    let file = |ruleset: &str, elements: &[Value]| json!({"format": "borealis-content/1", "ruleset": ruleset, "elements": elements});
    write_json(&dir.join("modern.json"), &file("dnd2024", &pack.0));
    write_json(&dir.join("legacy.json"), &file("dnd2014", &pack.1));
    write_json(
        &dir.join("pack.json"),
        &json!({"format": "borealis-pack/1", "id": "b55-test", "name": "B-55 test imports",
            "version": "1.0.0", "origin": "custom", "rulesets": ["dnd2014", "dnd2024"],
            "files": [{"path": "modern.json", "kind": "content"},
                      {"path": "legacy.json", "kind": "content"}]}),
    );
}

fn app(pack: Option<&Pack>) -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    if let Some(p) = pack {
        install(dir.path(), p);
    }
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

fn species(id: &str, name: &str, source: &str) -> Value {
    json!({"id": id, "name": name, "type": "Species", "source": source,
        "rules": [{"kind": "grant", "id": "ID_2024_TRAIT_DRAGONBORN_DRACONIC_ANCESTRY",
            "type": "Racial Trait"}]})
}

fn race(id: &str, name: &str) -> Value {
    json!({"id": id, "name": name, "type": "Race", "source": "Test Imports"})
}

fn third_dragonborn() -> Pack {
    Pack(
        vec![species(IMPORTED, "Dragonborn", "Test Imports")],
        Vec::new(),
    )
}

fn character(app: &AppState, ruleset: Ruleset) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Vera".into(),
        ruleset,
        starting_level: 1,
        gender: "f".into(),
        // manual scores: the contract generator types point-buy `costs` (a number-keyed
        // record) as an empty object, which no real snapshot matches
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    app.create_character(ruleset, &request).expect("create")
}

fn slot_of(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

fn species_slot(snap: &CharacterSnapshot) -> String {
    snap.slots
        .iter()
        .find(|s| matches!(s.element_type.as_str(), "race" | "species"))
        .expect("species slot")
        .id
        .clone()
}

/// The JSON page the UI receives (every row: no limit).
fn page(app: &AppState, id: &str, slot: &str, query: Option<&str>) -> Value {
    let page = app
        .get_slot_options(id, slot, query, None, None)
        .expect("options");
    serde_json::to_value(page).expect("json")
}

fn rows_named<'v>(page: &'v Value, name: &str) -> Vec<&'v Value> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|r| r["name"] == name)
        .collect()
}

fn version_ids(row: &Value) -> Vec<String> {
    row["versions"]
        .as_array()
        .map(|v| {
            v.iter()
                .map(|x| x["id"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn one_row<'v>(page: &'v Value, name: &str) -> &'v Value {
    let rows = rows_named(page, name);
    assert_eq!(rows.len(), 1, "{name} rows: {rows:#?}");
    rows[0]
}

#[test]
fn a_2024_species_list_shows_dragonborn_once_with_both_srd_versions() {
    let (_d, app) = app(None);
    let snap = character(&app, Ruleset::Dnd2024);
    let p = page(&app, &snap.id, &species_slot(&snap), None);
    let row = one_row(&p, "Dragonborn");
    assert_eq!(version_ids(row), vec![SRD24, SRD14]);
    assert_eq!(row["id"], SRD24, "the row shows its first version");
    assert_eq!(row["versions"][0], {
        let mut own = row.clone();
        own.as_object_mut().unwrap().remove("versions");
        own
    });
    assert_eq!(row["versions"][1]["edition"], "dnd2014");
    assert!(row["versions"][0].get("edition").is_none());
    assert_eq!(row["bundled"], true);
    let items = p["items"].as_array().unwrap();
    assert_eq!(
        p["total"].as_u64(),
        Some(items.len() as u64),
        "total counts rows"
    );
    let errs = Contract::load().response("get_slot_options", &p);
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn an_imported_version_comes_before_the_bundled_one_of_its_edition() {
    let (_d, app) = app(Some(&third_dragonborn()));
    let snap = character(&app, Ruleset::Dnd2024);
    let p = page(&app, &snap.id, &species_slot(&snap), None);
    let row = one_row(&p, "Dragonborn");
    assert_eq!(version_ids(row), vec![IMPORTED, SRD24, SRD14]);
    assert!(
        row["versions"][0].get("bundled").is_none(),
        "imported is not bundled"
    );
}

#[test]
fn the_chosen_version_leads_its_row() {
    let (_d, app) = app(None);
    let snap = character(&app, Ruleset::Dnd2024);
    let slot = species_slot(&snap);
    app.apply_selection(&snap.id, &slot, SRD14)
        .expect("2014 dragonborn");
    let p = page(&app, &snap.id, &slot, None);
    let row = one_row(&p, "Dragonborn");
    assert_eq!(row["id"], SRD14);
    assert_eq!(version_ids(row), vec![SRD14, SRD24]);
}

#[test]
fn a_superseded_other_edition_class_is_no_version() {
    let (_d, app) = app(None);
    let snap = character(&app, Ruleset::Dnd2024);
    let p = page(&app, &snap.id, &slot_of(&snap, "Class"), None);
    let row = one_row(&p, "Fighter");
    assert!(row.get("versions").is_none(), "{row:#}");
}

fn wizard_2014(app: &AppState) -> CharacterSnapshot {
    let snap = character(app, Ruleset::Dnd2014);
    let id = snap.id.clone();
    let s = app
        .apply_selection(&id, &species_slot(&snap), "ID_RACE_HUMAN")
        .expect("human");
    let mut s = app
        .apply_selection(&id, &slot_of(&s, "Class"), "ID_WOTC_PHB_CLASS_WIZARD")
        .expect("wizard");
    for _ in 1..5 {
        s = app.level_up(&id, "main").expect("level up");
    }
    s
}

/// Fireball rows of every slot of the character that lists Fireball.
fn fireball_rows(app: &AppState, snap: &CharacterSnapshot) -> Vec<Value> {
    snap.slots
        .iter()
        .filter(|s| s.element_type == "spell")
        .flat_map(|s| {
            let p = page(app, &snap.id, &s.id, Some("fireball"));
            rows_named(&p, "Fireball")
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_2014_spell_slot_groups_the_two_fireballs_unless_strict() {
    let (_d, app) = app(None);
    let snap = wizard_2014(&app);
    let open = app
        .set_allow_cross_edition(&snap.id, Some(true))
        .expect("strict off");
    let rows = fireball_rows(&app, &open);
    assert!(!rows.is_empty(), "a wizard slot offers Fireball");
    for row in &rows {
        assert_eq!(
            version_ids(row),
            vec!["ID_PHB_SPELL_FIREBALL", "ID_2024_SPELL_FIREBALL"],
            "{row:#}"
        );
    }
    let strict = app
        .set_allow_cross_edition(&snap.id, Some(false))
        .expect("strict on");
    let rows = fireball_rows(&app, &strict);
    assert!(!rows.is_empty());
    for row in &rows {
        assert!(row.get("versions").is_none(), "{row:#}");
    }
}

#[test]
fn names_group_after_whitespace_case_and_quote_folding_only() {
    let pack = Pack(
        Vec::new(),
        vec![
            race("ID_T_RACE_HALF_ELF_SPACED", " half-elf "),
            race("ID_T_RACE_HALF_ELF_DASH", "Half\u{2010}Elf"),
            race("ID_T_RACE_FOLK_CURLY", "Test\u{2019}s Folk"),
            race("ID_T_RACE_FOLK_PLAIN", "test's  folk"),
        ],
    );
    let (_d, app) = app(Some(&pack));
    let snap = character(&app, Ruleset::Dnd2014);
    let slot = species_slot(&snap);
    let p = page(&app, &snap.id, &slot, None);
    let half: Vec<_> = p["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| version_ids(r).contains(&"ID_RACE_HALFELF".to_string()))
        .collect();
    assert_eq!(half.len(), 1, "{half:#?}");
    assert_eq!(
        version_ids(half[0]),
        vec!["ID_T_RACE_HALF_ELF_SPACED", "ID_RACE_HALFELF"]
    );
    assert_eq!(
        half[0]["name"], " half-elf ",
        "the row shows its first version's own name"
    );
    let dash = one_row(&p, "Half\u{2010}Elf");
    assert!(
        dash.get("versions").is_none(),
        "another dash is another name"
    );
    let curly = page(&app, &snap.id, &slot, Some("TEST'S FOLK"));
    assert_eq!(curly["total"], 1, "{curly:#}");
    assert_eq!(
        version_ids(&curly["items"][0]),
        vec!["ID_T_RACE_FOLK_CURLY", "ID_T_RACE_FOLK_PLAIN"]
    );
    let by_curly_query = page(&app, &snap.id, &slot, Some("test\u{2019}s"));
    assert_eq!(by_curly_query["total"], 1);
}

fn many_choices() -> Pack {
    let mut modern = vec![json!({"id": MANY, "name": "Many Folk", "type": "Species",
        "source": "Test Imports", "rules": [{"kind": "select", "type": "Racial Trait",
            "name": "Test Choice", "supports": "Test Many"}]})];
    modern.extend((1..=MANY_ROWS).map(|i| {
        json!({"id": format!("ID_T_TRAIT_CHOICE_{i:03}"), "name": format!("Choice {i:03}"),
            "type": "Racial Trait", "source": "Test Imports", "supports": ["Test Many"]})
    }));
    Pack(modern, Vec::new())
}

#[test]
fn every_row_of_a_long_slot_is_listed_and_total_counts_rows() {
    let (_d, app) = app(Some(&many_choices()));
    let snap = character(&app, Ruleset::Dnd2024);
    let snap = app
        .apply_selection(&snap.id, &species_slot(&snap), MANY)
        .expect("many folk");
    let slot = slot_of(&snap, "Test Choice");
    let all = app
        .get_slot_options(&snap.id, &slot, None, None, None)
        .expect("all");
    assert_eq!((all.total, all.items.len()), (MANY_ROWS, MANY_ROWS));
    let zero = app
        .get_slot_options(&snap.id, &slot, None, Some(0), None)
        .expect("zero");
    assert_eq!(zero.items.len(), MANY_ROWS, "limit 0 lists every row");
    let tail = app
        .get_slot_options(&snap.id, &slot, None, Some(100), Some(200))
        .expect("page");
    assert_eq!((tail.total, tail.items.len()), (MANY_ROWS, 50));
}

fn chosen_id(snap: &CharacterSnapshot, title: &str) -> Option<String> {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .and_then(|s| s.chosen.as_ref())
        .map(|c| c.id.clone())
}

#[test]
fn switching_versions_keeps_sub_choices_that_fit_and_reports_the_rest() {
    let (_d, app) = app(Some(&third_dragonborn()));
    let snap = character(&app, Ruleset::Dnd2024);
    let id = snap.id.clone();
    let slot = species_slot(&snap);
    let s = app
        .apply_selection(&id, &slot, SRD14)
        .expect("2014 dragonborn");
    app.apply_selection(&id, &slot_of(&s, "Draconic Ancestry"), RED14)
        .expect("red 2014");

    let out = app.switch_version(&id, &slot, SRD24).expect("to 2024");
    assert_eq!(out.dropped, vec!["Draconic Ancestry".to_string()]);
    assert_eq!(chosen_id(&out.snapshot, "Draconic Ancestry"), None);
    let value = serde_json::to_value(&out).expect("json");
    assert_eq!(value["dropped"], json!(["Draconic Ancestry"]));
    let errs = Contract::load().response("switch_version", &value);
    assert!(errs.is_empty(), "{errs:?}");

    let s = app
        .apply_selection(&id, &slot_of(&out.snapshot, "Draconic Ancestry"), RED24)
        .expect("red 2024");
    assert_eq!(
        s.slots
            .iter()
            .find(|x| x.id == slot)
            .and_then(|x| x.chosen.as_ref())
            .map(|c| c.id.as_str()),
        Some(SRD24)
    );
    let out = app
        .switch_version(&id, &slot, IMPORTED)
        .expect("to imported");
    assert!(out.dropped.is_empty(), "{:?}", out.dropped);
    assert_eq!(
        chosen_id(&out.snapshot, "Draconic Ancestry").as_deref(),
        Some(RED24)
    );
}

#[test]
fn switching_to_an_element_of_another_row_is_refused_and_changes_nothing() {
    let (_d, app) = app(None);
    let snap = character(&app, Ruleset::Dnd2024);
    let id = snap.id.clone();
    let slot = species_slot(&snap);
    let before = app.apply_selection(&id, &slot, SRD24).expect("dragonborn");
    let err = app
        .switch_version(&id, &slot, "ID_2024_SPECIES_ELF")
        .expect_err("elf is no version of dragonborn");
    assert_eq!(err.code, ErrorCode::RequirementFailed);
    assert!(
        err.message.contains("not a version of Dragonborn"),
        "{}",
        err.message
    );
    let after = app.get_snapshot(&id).expect("snapshot");
    assert_eq!(
        serde_json::to_value(&before.slots).unwrap(),
        serde_json::to_value(&after.slots).unwrap()
    );
}

#[test]
fn an_equivalent_twin_is_listed_and_accepted_while_its_counterpart_is_chosen() {
    let mut twin = species(TWIN, "Dragonborn", "Test Twins");
    twin["maps_to"] = json!({"dnd2024": SRD24, "kind": "equivalent"});
    let (_d, app) = app(Some(&Pack(vec![twin], Vec::new())));
    let snap = character(&app, Ruleset::Dnd2024);
    let id = snap.id.clone();
    let slot = species_slot(&snap);
    let s = app.apply_selection(&id, &slot, SRD24).expect("dragonborn");
    app.apply_selection(&id, &slot_of(&s, "Draconic Ancestry"), RED24)
        .expect("red");
    let p = page(&app, &id, &slot, None);
    let row = one_row(&p, "Dragonborn");
    assert_eq!(version_ids(row), vec![SRD24, TWIN, SRD14]);
    let out = app.switch_version(&id, &slot, TWIN).expect("twin");
    assert!(out.dropped.is_empty(), "{:?}", out.dropped);
    assert_eq!(
        chosen_id(&out.snapshot, "Draconic Ancestry").as_deref(),
        Some(RED24)
    );
}

#[test]
fn two_same_name_subraces_of_one_race_are_one_row_with_two_versions() {
    // U10a: subraces group like other rows (synthetic race, two "Sub Race" entries of one name).
    let kin = json!({"id": "ID_T_RACE_KIN", "name": "Test Kin", "type": "Race", "source": "Test Imports",
        "rules": [{"kind": "select", "type": "Sub Race", "name": "Kin Subrace", "supports": "Test Kin"}]});
    let sub = |id: &str, source: &str| json!({"id": id, "name": "Hill Folk", "type": "Sub Race", "source": source, "supports": ["Test Kin"]});
    let pack = Pack(
        Vec::new(),
        vec![
            kin,
            sub("ID_T_SUB_RACE_HILL_FOLK", "Test Imports"),
            sub("ID_T_SUB_RACE_HILL_FOLK_TWO", "Test Imports Two"),
        ],
    );
    let (_d, app) = app(Some(&pack));
    let snap = character(&app, Ruleset::Dnd2014);
    let s = app
        .apply_selection(&snap.id, &species_slot(&snap), "ID_T_RACE_KIN")
        .expect("kin");
    let p = page(&app, &s.id, &slot_of(&s, "Kin Subrace"), None);
    let row = one_row(&p, "Hill Folk");
    assert_eq!(
        version_ids(row),
        vec!["ID_T_SUB_RACE_HILL_FOLK", "ID_T_SUB_RACE_HILL_FOLK_TWO"]
    );
    assert_eq!(p["total"], 1);
    let errs = Contract::load().response("get_slot_options", &p);
    assert!(errs.is_empty(), "{errs:?}");
}
