//! Multiclass picker on the real backend (regression): `Multiclass` elements are
//! derived by the engine universe and never live in the content store, so the picker must resolve
//! them through the hub, not the store ("Nothing matches" for every class otherwise).

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::CompendiumQuery;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

struct Build {
    ruleset: Ruleset,
    species: &'static str,
    class: &'static str,
    background: &'static str,
}

const WIZARD_2024: Build = Build {
    ruleset: Ruleset::Dnd2024,
    species: "ID_2024_SPECIES_ELF",
    class: "ID_2024_CLASS_WIZARD",
    background: "ID_2024_BACKGROUND_SAGE",
};

const WIZARD_2014: Build = Build {
    ruleset: Ruleset::Dnd2014,
    species: "ID_RACE_ELF",
    class: "ID_WOTC_PHB_CLASS_WIZARD",
    background: "ID_BACKGROUND_ACOLYTE",
};

/// A Wizard 1 with STR 13 / INT 13 and multiclassing on, then NEW MULTICLASS (an empty level 2).
fn wizard_with_new_multiclass(app: &AppState, b: &Build) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Seraphine".into(),
        ruleset: b.ruleset,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let s = app.create_character(b.ruleset, &request).expect("create");
    let id = s.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&s, "Race"), b.species)
        .expect("species");
    let s = app
        .apply_selection(&id, &slot_id(&s, "Class"), b.class)
        .expect("class");
    app.apply_selection(&id, &slot_id(&s, "Background"), b.background)
        .expect("background");
    app.set_character_options(
        &id,
        &OptionsPatch {
            multiclassing: Some(true),
            ..OptionsPatch::default()
        },
    )
    .expect("options");
    app.set_base_score(&id, "str", 13).expect("str");
    let s = app.set_base_score(&id, "int", 13).expect("int");
    assert!(
        s.level.can_multiclass,
        "{:?}",
        s.level.multiclass_blocked_reason
    );
    app.add_multiclass(&id).expect("new multiclass")
}

fn multiclass_options(
    app: &AppState,
    snap: &CharacterSnapshot,
) -> (String, Vec<(String, Option<String>)>) {
    let slot = slot_id(snap, "Multiclass (Level 2)");
    let page = app
        .get_slot_options(&snap.id, &slot, None, None, None)
        .expect("options");
    let rows = page
        .items
        .into_iter()
        .map(|o| (o.summary.id, o.summary.prerequisites))
        .collect();
    (slot, rows)
}

fn assert_picker_offers_fighter(b: &Build, fighter: &str, bard: &str, own: &str) {
    let (_dir, app) = app();
    let snap = wizard_with_new_multiclass(&app, b);
    let (slot, rows) = multiclass_options(&app, &snap);
    let ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&fighter), "{fighter} missing from {ids:?}");
    assert!(
        !ids.contains(&own),
        "the main class is never a multiclass option: {ids:?}"
    );
    assert!(
        !ids.contains(&bard),
        "CHA 8 fails the Bard prerequisite: {ids:?}"
    );
    let prereq = rows
        .iter()
        .find(|(id, _)| id == fighter)
        .and_then(|(_, p)| p.clone());
    assert!(
        prereq.as_deref().is_some_and(|p| p.contains("Strength 13")),
        "prerequisite text on the row: {prereq:?}"
    );
    app.get_element(fighter)
        .expect("describe pane resolves the multiclass entry");
    let after = app
        .apply_selection(&snap.id, &slot, fighter)
        .expect("pick Fighter");
    let names: Vec<_> = after
        .level
        .classes
        .iter()
        .filter_map(|c| c.name.clone())
        .collect();
    assert_eq!(names, ["Wizard", "Fighter"], "{names:?}");
}

#[test]
fn multiclass_picker_offers_classes_2024() {
    assert_picker_offers_fighter(
        &WIZARD_2024,
        "ID_2024_MULTICLASS_FIGHTER",
        "ID_2024_MULTICLASS_BARD",
        "ID_2024_MULTICLASS_WIZARD",
    );
}

#[test]
fn multiclass_picker_offers_classes_2014() {
    assert_picker_offers_fighter(
        &WIZARD_2014,
        "ID_WOTC_PHB_MULTICLASS_FIGHTER",
        "ID_WOTC_PHB_MULTICLASS_BARD",
        "ID_WOTC_PHB_MULTICLASS_WIZARD",
    );
}

#[test]
fn compendium_lists_multiclass_entries_only_on_request() {
    let (_dir, app) = app();
    let query = |types: Option<Vec<String>>| CompendiumQuery {
        query: "Fighter".into(),
        types,
        sources: None,
        ruleset: Some("dnd2024".into()),
        limit: 50,
        offset: 0,
    };
    let multi = app
        .search_compendium(&query(Some(vec!["multiclass".into()])))
        .expect("search");
    let fighter = multi
        .items
        .iter()
        .find(|e| e.id == "ID_2024_MULTICLASS_FIGHTER")
        .expect("level-up flyout reads the Fighter multiclass entry");
    assert_eq!(
        fighter.prerequisites.as_deref(),
        Some("Strength 13 or Dexterity 13")
    );
    let plain = app.search_compendium(&query(None)).expect("search");
    assert!(
        plain.items.iter().all(|e| e.ty != "multiclass"),
        "multiclass entries stay hidden from the normal compendium"
    );
}
