//! The 5etools import through the real app state (no GUI), on the importer's SRD-only fixtures
//! plus an invented homebrew file: packs install as `local-import`, never redistributable; the
//! store reloads and an imported class, subclass and species are pickable through the same
//! builder API the UI uses; a re-import replaces the packs without duplicates and keeps an open
//! character valid; a homebrew `_copy` of a PHB entry resolves.

use borealis_lib::content_dto::{ContentPack, FiveToolsImportRequest, PackKind};
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-import-5etools/tests/fixtures/data")
}

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn folder_token(app: &AppState, dir: &Path) -> String {
    app.picked
        .grant(dir.to_path_buf(), PickPurpose::FivetoolsFolder)
        .expect("grant")
        .token
}

fn import(app: &AppState, folder: &str, sources: &[&str]) -> Vec<ContentPack> {
    let request = FiveToolsImportRequest {
        folder: folder.to_string(),
        sources: sources.iter().map(|s| (*s).to_string()).collect(),
        include_fluff: true,
    };
    app.import_5etools(&request, &NullSink)
        .expect("import")
        .packs
}

fn five_etools_packs(app: &AppState) -> Vec<ContentPack> {
    app.list_packs()
        .expect("packs")
        .into_iter()
        .filter(|p| p.id.starts_with("5etools-"))
        .collect()
}

fn slot<'a>(snap: &'a CharacterSnapshot, element_type: &str) -> &'a str {
    snap.slots
        .iter()
        .find(|s| s.element_type.eq_ignore_ascii_case(element_type) && s.chosen.is_none())
        .unwrap_or_else(|| {
            let all: Vec<_> = snap
                .slots
                .iter()
                .map(|s| (s.title.as_str(), s.element_type.as_str()))
                .collect();
            panic!("no open {element_type} slot in {all:?}")
        })
        .id
        .as_str()
}

fn offered(app: &AppState, snap: &CharacterSnapshot, element_type: &str) -> Vec<String> {
    app.get_slot_options(&snap.id, slot(snap, element_type), None, Some(500), None)
        .expect("options")
        .items
        .into_iter()
        .map(|o| o.summary.id)
        .collect()
}

fn new_character(app: &AppState, ruleset: Ruleset) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Probe".into(),
        ruleset,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    app.create_character(ruleset, &request).expect("create")
}

/// Species, class, then levels to 3 and the subclass, all through the builder API.
fn build(
    app: &AppState,
    ruleset: Ruleset,
    species: &str,
    class: &str,
    subclass: &str,
) -> CharacterSnapshot {
    let s = new_character(app, ruleset);
    let id = s.id.clone();
    let species_type = if s
        .slots
        .iter()
        .any(|x| x.element_type.eq_ignore_ascii_case("Species"))
    {
        "Species"
    } else {
        "Race"
    };
    assert!(
        offered(app, &s, species_type).iter().any(|o| o == species),
        "{species} not offered"
    );
    let s = app
        .apply_selection(&id, slot(&s, species_type), species)
        .expect("species");
    assert!(
        offered(app, &s, "Class").iter().any(|o| o == class),
        "{class} not offered"
    );
    let mut s = app
        .apply_selection(&id, slot(&s, "Class"), class)
        .expect("class");
    while s.level.total < 3 {
        s = app.level_up(&id, "main").expect("level up");
    }
    // the subclass may already be chosen by the slot's `default` (a sole candidate never is)
    let auto = s.slots.iter().any(|x| {
        x.element_type.eq_ignore_ascii_case("Archetype")
            && x.chosen.as_ref().is_some_and(|c| c.id == subclass)
    });
    let s = if auto {
        s
    } else {
        assert!(
            offered(app, &s, "Archetype").iter().any(|o| o == subclass),
            "{subclass} not offered"
        );
        app.apply_selection(&id, slot(&s, "Archetype"), subclass)
            .expect("subclass")
    };
    let main = &s.level.classes[0];
    assert_eq!(main.class_id.as_deref(), Some(class));
    assert_eq!(
        main.subclass.as_ref().map(|r| r.id.as_str()),
        Some(subclass)
    );
    s
}

#[test]
fn imported_packs_install_reload_and_are_pickable_in_both_editions() {
    let (_dir, app) = app();
    let folder = folder_token(&app, &fixtures());
    let scan = app.scan_5etools_folder(&folder).expect("scan");
    assert!(scan
        .sources
        .iter()
        .any(|s| s.abbr == "XPHB" && s.ruleset == "dnd2024"));

    let packs = import(&app, &folder, &["PHB", "XPHB"]);
    assert_eq!(packs.len(), 2);
    for p in &packs {
        assert_eq!(p.kind, PackKind::LocalImport, "{}", p.id);
        assert_eq!(p.redistributable, Some(false), "{}", p.id);
        assert!(p.enabled && p.element_count > 0);
    }

    let old = build(
        &app,
        Ruleset::Dnd2014,
        "ID_5ET_PHB_RACE_DWARF",
        "ID_5ET_PHB_CLASS_FIGHTER",
        "ID_5ET_PHB_SUBCLASS_FIGHTER_CHAMPION",
    );
    let new = build(
        &app,
        Ruleset::Dnd2024,
        "ID_5ET_XPHB_SPECIES_DWARF",
        "ID_5ET_XPHB_CLASS_WIZARD",
        "ID_5ET_XPHB_SUBCLASS_WIZARD_EVOKER",
    );
    assert_eq!(new.level.total, 3);
    assert!(
        !new.spellcasting.classes.is_empty(),
        "the imported 2024 wizard casts spells"
    );

    // Re-import the same books: packs are replaced in place, not duplicated, and the open
    // characters still resolve every imported choice.
    let before: Vec<(String, usize)> = five_etools_packs(&app)
        .into_iter()
        .map(|p| (p.id, p.element_count))
        .collect();
    let again = import(&app, &folder, &["PHB", "XPHB"]);
    assert_eq!(again.len(), 2);
    let after: Vec<(String, usize)> = five_etools_packs(&app)
        .into_iter()
        .map(|p| (p.id, p.element_count))
        .collect();
    assert_eq!(before, after, "re-import must replace, not duplicate");
    for snap in [&old, &new] {
        let reloaded = app.get_snapshot(&snap.id).expect("snapshot");
        assert_eq!(
            reloaded.level.classes[0].class_id,
            snap.level.classes[0].class_id
        );
        assert_eq!(
            reloaded.level.classes[0]
                .subclass
                .as_ref()
                .map(|r| r.id.clone()),
            snap.level.classes[0]
                .subclass
                .as_ref()
                .map(|r| r.id.clone())
        );
    }
}

#[test]
fn a_homebrew_file_copying_a_phb_entry_imports_as_its_own_pack() {
    let data = tempfile::tempdir().expect("data dir");
    copy_tree(&fixtures(), data.path());
    let brew = serde_json::json!({
        "_meta": {"sources": [{"json": "HBX", "full": "Probe Homebrew"}]},
        "race": [{
            "name": "Stoneborn Dwarf", "source": "HBX",
            "_copy": {"name": "Dwarf", "source": "PHB", "_mod": {
                "entries": {"mode": "appendArr", "items": {"type": "entries",
                    "name": "Stone Sense", "entries": ["You can feel the stone."]}}}}
        }],
        "subrace": [{
            "name": "Deep", "source": "HBX", "raceName": "Dwarf", "raceSource": "PHB",
            "ability": [{"int": 2}], "overwrite": {"ability": true},
            "entries": [{"type": "entries", "name": "Deep Mind", "entries": ["Clever."]}]
        }]
    });
    std::fs::create_dir_all(data.path().join("homebrew")).unwrap();
    std::fs::write(data.path().join("homebrew/probe.json"), brew.to_string()).unwrap();

    let (_dir, app) = app();
    let folder = folder_token(&app, data.path());
    let scan = app.scan_5etools_folder(&folder).expect("scan");
    let hbx = scan.sources.iter().find(|s| s.abbr == "HBX").expect("HBX");
    assert_eq!(hbx.name, "Probe Homebrew");
    let packs = import(&app, &folder, &["HBX"]);
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].id, "5etools-hbx");
    assert_eq!(packs[0].redistributable, Some(false));
    let s = new_character(&app, Ruleset::Dnd2014);
    let species = offered(&app, &s, "Race");
    assert!(
        species
            .iter()
            .any(|id| id == "ID_5ET_HBX_RACE_STONEBORN_DWARF"),
        "the homebrew race resolved its _copy and is offered"
    );
    let s = app
        .apply_selection(&s.id, slot(&s, "Race"), "ID_5ET_HBX_RACE_STONEBORN_DWARF")
        .expect("pick homebrew race");
    // the copied dwarf traits came along (speed 25 from the PHB dwarf)
    assert_eq!(s.stats.speed, 25);

    // A homebrew subrace that overwrites the PHB dwarf's ability bonus: with both books
    // imported, choosing it switches the parent's +2 CON off.
    let packs = import(&app, &folder, &["PHB", "HBX"]);
    assert_eq!(packs.len(), 2);
    let s = new_character(&app, Ruleset::Dnd2014);
    let s = app
        .apply_selection(&s.id, slot(&s, "Race"), "ID_5ET_PHB_RACE_DWARF")
        .expect("dwarf");
    assert_eq!((s.abilities.bonus.con, s.abilities.bonus.int), (2, 0));
    let s = app
        .apply_selection(&s.id, slot(&s, "Subrace"), "ID_5ET_HBX_SUBRACE_DWARF_DEEP")
        .expect("deep dwarf");
    assert_eq!((s.abilities.bonus.con, s.abilities.bonus.int), (0, 2));
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let target = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_tree(&p, &target);
        } else {
            std::fs::copy(&p, &target).unwrap();
        }
    }
}
