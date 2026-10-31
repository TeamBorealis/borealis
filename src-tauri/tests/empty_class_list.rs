//! GitHub #10 (decision U14) through the real command layer: a Class list that is empty because
//! of the edition says so with counts (R4 stands: classes never cross editions), the generic
//! "source restrictions" cause is reported only when restrictions really hide options, the New
//! Character dialog learns how many classes each edition offers, and "Start over as 2014" makes a
//! NEW 2014 character while the old one stays as it was on disk. Every response is checked against
//! the TS contract. The live case (AuroraLegacy core.index, the reporter's setup) is gated on
//! `BOREALIS_LIVE_INDEX_URL` (`BOREALIS_LIVE_DATA_DIR` keeps its download).

mod contract_support;
use base64::Engine as _;
use borealis_lib::content_index::HttpsFetcher;
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde_json::{json, Value};
use std::path::PathBuf;

struct App {
    state: AppState,
    contract: Contract,
    dir: Option<tempfile::TempDir>,
}

impl App {
    fn at(data: Option<PathBuf>) -> Self {
        let (dir, data) = match data {
            Some(d) => (None, d),
            None => {
                let dir = tempfile::tempdir().expect("temp dir");
                let path = dir.path().to_path_buf();
                (Some(dir), path)
            }
        };
        let paths = AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data,
        };
        Self {
            state: AppState::new(paths).expect("state"),
            contract: Contract::load(),
            dir,
        }
    }

    fn checked(&self, command: &str, snap: &CharacterSnapshot) -> Value {
        let v = serde_json::to_value(snap).expect("json");
        let errors = self.contract.response(command, &v);
        assert!(errors.is_empty(), "{command}: {errors:?}");
        v
    }

    fn create(&self, ruleset: Ruleset, options: OptionsPatch) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Ayla Thornwick".into(),
            ruleset,
            starting_level: 1,
            gender: "Female".into(),
            ability_method: AbilityMethodDto::Manual,
            options,
            portrait: None,
            group: Some("Table Two".into()),
        };
        let snap = self
            .state
            .create_character(ruleset, &request)
            .expect("create");
        self.checked("create_character", &snap);
        snap
    }

    fn sources_but(&self, out: &[&str]) -> Vec<String> {
        self.state
            .list_sources()
            .expect("sources")
            .into_iter()
            .map(|s| s.id)
            .filter(|s| !out.contains(&s.as_str()))
            .collect()
    }
}

fn class_slot(v: &Value) -> Value {
    v["slots"]
        .as_array()
        .expect("slots")
        .iter()
        .find(|s| s["title"] == "Class")
        .cloned()
        .expect("class slot")
}

fn class_rows(app: &App, snap: &CharacterSnapshot) -> usize {
    let slot = snap
        .slots
        .iter()
        .find(|s| s.title == "Class")
        .expect("class slot");
    app.state
        .get_slot_options(&snap.id, &slot.id, None, None, None)
        .expect("options")
        .total
}

#[test]
fn an_edition_emptied_class_list_says_why_with_counts() {
    let app = App::at(None);
    let snap = app.create(Ruleset::Dnd2024, OptionsPatch::default());
    let without = app.sources_but(&["SRD 5.2.1"]);
    let snap = app
        .state
        .set_source_restrictions(&snap.id, Some(&without))
        .expect("restrict");
    let v = app.checked("set_source_restrictions", &snap);
    assert_eq!(class_rows(&app, &snap), 0);
    assert_eq!(
        class_slot(&v)["emptyReason"],
        json!({"otherEdition": {"edition": "dnd2014", "rejected": 12, "notAllowed": 0}, "restricted": 12}),
        "the 12 SRD 5.1 classes need a 2014 character; the 12 SRD 5.2.1 ones are restricted"
    );
    // with the other edition allowed nothing changes: classes never cross (R4)
    let snap = app
        .state
        .set_allow_cross_edition(&snap.id, Some(true))
        .expect("cross");
    assert_eq!(class_rows(&app, &snap), 0);
    assert_eq!(
        class_slot(&app.checked("set_allow_cross_edition", &snap))["emptyReason"]["otherEdition"]
            ["rejected"],
        12
    );
}

#[test]
fn a_class_list_with_classes_has_no_reason() {
    let app = App::at(None);
    for rs in [Ruleset::Dnd2024, Ruleset::Dnd2014] {
        let snap = app.create(rs, OptionsPatch::default());
        let v = app.checked("create_character", &snap);
        assert_eq!(class_rows(&app, &snap), 12);
        assert_eq!(class_slot(&v)["emptyReason"], Value::Null, "{rs:?}");
    }
}

#[test]
fn only_restrictions_name_only_restrictions() {
    let app = App::at(None);
    let snap = app.create(Ruleset::Dnd2014, OptionsPatch::default());
    let without = app.sources_but(&["SRD 5.1"]);
    let snap = app
        .state
        .set_source_restrictions(&snap.id, Some(&without))
        .expect("restrict");
    let v = app.checked("set_source_restrictions", &snap);
    // the 12 SRD 5.2.1 classes are 2024 (R4); the 12 SRD 5.1 ones are only restricted
    assert_eq!(
        class_slot(&v)["emptyReason"],
        json!({"otherEdition": {"edition": "dnd2024", "rejected": 12, "notAllowed": 0}, "restricted": 12})
    );
}

#[test]
fn the_new_character_dialog_learns_the_classes_per_edition() {
    let app = App::at(None);
    let counts = app.state.class_counts_by_edition().expect("counts");
    let v = serde_json::to_value(counts).expect("json");
    assert!(app
        .contract
        .response("class_counts_by_edition", &v)
        .is_empty());
    assert_eq!(v, json!({"dnd2014": 12, "dnd2024": 12}));
    app.state
        .set_pack_enabled("srd-5.2.1", false, &NullSink)
        .expect("switch off SRD 5.2.1");
    let v = serde_json::to_value(app.state.class_counts_by_edition().expect("counts")).unwrap();
    assert_eq!(v, json!({"dnd2014": 12, "dnd2024": 0}));
}

#[test]
fn start_over_as_2014_makes_a_new_character_and_keeps_the_old_one() {
    let app = App::at(None);
    let old = app.create(
        Ruleset::Dnd2024,
        OptionsPatch {
            feats: Some(false),
            average_hp: Some(false),
            ..OptionsPatch::default()
        },
    );
    let without = app.sources_but(&["SRD 5.2.1"]);
    app.state
        .set_source_restrictions(&old.id, Some(&without))
        .expect("restrict");
    app.state
        .save_character(&old.id)
        .expect("save the 2024 character");
    let old_file = app.state.character_path(&old.id).expect("path");
    let before = std::fs::read(&old_file).expect("old file");

    let fresh = app
        .state
        .start_over_in_edition(&old.id, Ruleset::Dnd2014)
        .expect("start over");
    let v = app.checked("start_over_in_edition", &fresh);
    assert_ne!(
        fresh.id, old.id,
        "a new character, not the old one rewritten"
    );
    assert_eq!(fresh.ruleset, Ruleset::Dnd2014);
    assert_eq!(fresh.name, "Ayla Thornwick");
    assert_eq!(fresh.group, "Table Two");
    assert_eq!(fresh.details.gender, "Female");
    assert!(!fresh.options.feats && !fresh.options.average_hp && fresh.options.multiclassing);
    assert_eq!(fresh.source_restrictions, Some(without.clone()));
    assert_eq!(class_rows(&app, &fresh), 12, "the 2014 classes are offered");
    assert_eq!(class_slot(&v)["emptyReason"], Value::Null);

    // the old character is untouched on disk and both are in the collection
    assert_eq!(std::fs::read(&old_file).expect("old file"), before);
    let names: Vec<_> = app
        .state
        .list_characters()
        .expect("library")
        .into_iter()
        .filter(|c| c.name == "Ayla Thornwick")
        .map(|c| (c.id, c.ruleset))
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&(old.id.clone(), Ruleset::Dnd2024)));
    assert!(names.contains(&(fresh.id.clone(), Ruleset::Dnd2014)));
    assert!(app.state.character_path(&fresh.id).expect("path").is_file());

    // the same edition, or "any", is refused
    for rs in [Ruleset::Dnd2024, Ruleset::Any] {
        let err = app.state.start_over_in_edition(&old.id, rs).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidArgument, "{rs:?}");
    }
    let err = app
        .state
        .start_over_in_edition("no-such-character", Ruleset::Dnd2014)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    drop(app.dir);
}

/// The reporter's setup: AuroraLegacy core, a 2024 character with every 2024 and 2025 source off.
#[test]
fn live_auroralegacy_2024_character_without_2024_sources() {
    let Ok(url) = std::env::var("BOREALIS_LIVE_INDEX_URL") else {
        eprintln!("SKIPPED: set BOREALIS_LIVE_INDEX_URL");
        return;
    };
    let kept = std::env::var("BOREALIS_LIVE_DATA_DIR")
        .ok()
        .map(PathBuf::from);
    let app = App::at(kept);
    if app.state.list_indexes().is_empty() {
        app.state.add_index(&url).expect("add index");
        app.state
            .update_indexes(&HttpsFetcher, &NullSink)
            .expect("update");
    }
    let modern = |s: &str| s.contains("2024") || s.contains("2025") || s.contains("5.2");
    let all: Vec<String> = app
        .state
        .list_sources()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    let old_books: Vec<String> = all.iter().filter(|s| !modern(s)).cloned().collect();
    assert!(
        all.iter().any(|s| s == "Player’s Handbook (2024)"),
        "{all:?}"
    );

    let snap = app.create(Ruleset::Dnd2024, OptionsPatch::default());
    let snap = app
        .state
        .set_source_restrictions(&snap.id, Some(&old_books))
        .expect("restrict");
    let v = app.checked("set_source_restrictions", &snap);
    assert_eq!(class_rows(&app, &snap), 0);
    let reason = class_slot(&v)["emptyReason"].clone();
    println!("live empty reason: {reason}");
    assert_eq!(reason["otherEdition"]["edition"], "dnd2014");
    assert_eq!(
        reason["otherEdition"]["rejected"], 12,
        "the 12 PHB (2014) classes"
    );
    assert!(
        reason["restricted"].as_u64().unwrap() >= 12,
        "PHB 2024 / SRD 5.2.1 classes"
    );

    let fresh = app
        .state
        .start_over_in_edition(&snap.id, Ruleset::Dnd2014)
        .expect("start over");
    assert_eq!(class_rows(&app, &fresh), 12);
    for id in [snap.id, fresh.id] {
        app.state.delete_character(&id).expect("clean up");
    }
}

/// Review M1: Start over copies the portrait as it is stored. An Aurora import keeps a JPEG under
/// `portrait.png`, which a data-URL round trip refuses ("does not match its declared type"), so
/// starting over failed for such a character.
#[test]
fn start_over_copies_an_imported_portrait_as_it_is() {
    let app = App::at(None);
    let old = app.create(Ruleset::Dnd2024, OptionsPatch::default());
    app.state.save_character(&old.id).expect("save");
    let path = app.state.character_path(&old.id).expect("path");
    // as the Aurora importer stores it: JPEG bytes, file name portrait.png
    let jpeg = base64::engine::general_purpose::STANDARD.encode([
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01,
    ]);
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["portrait"] = json!({"file_name": "portrait.png", "data_base64": jpeg});
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
    app.state.close_character(&old.id, true).expect("close");
    app.state.load_character(&old.id).expect("reload");

    let fresh = app
        .state
        .start_over_in_edition(&old.id, Ruleset::Dnd2014)
        .expect("start over with an imported portrait");
    let saved: Value = serde_json::from_str(
        &std::fs::read_to_string(app.state.character_path(&fresh.id).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["portrait"], v["portrait"]);
}
