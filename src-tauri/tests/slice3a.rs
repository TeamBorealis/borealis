//! Service tests: Manage commands, pack management, imports (Aurora XML, 5etools) and
//! update indexes, all against the real bundled content in a temporary data directory.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::{
    AttackInput, ContentProgress, FiveToolsImportRequest, ImportPackRequest, MoveDirection,
    PackKind,
};
use borealis_lib::content_index::{sha256_hex, Fetcher};
use borealis_lib::content_packs::{EventSink, NullSink};
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::CompendiumQuery;
use borealis_lib::error::{ApiError, ErrorCode};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

const TEST_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>Test Book</name>
    <update version="1.0.0"><file name="test-book.xml" url="https://example.invalid/test-book.xml"/></update>
  </info>
  <element name="Zephyr Gift" type="Feat" source="Test Book" id="ID_TEST_FEAT_ZEPHYR_GIFT">
    <description><p>A hand-written feat for the tests.</p></description>
    <sheet display="true"><description>Test feat.</description></sheet>
    <setters><set name="category">General</set></setters>
  </element>
</elements>"#;

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<(String, String)>>,
}

impl Recorder {
    fn names(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.0.clone())
            .collect()
    }
    fn phases(&self, kind: &str) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.0 == kind)
            .map(|e| e.1.clone())
            .collect()
    }
}

impl EventSink for Recorder {
    fn content_progress(&self, p: &ContentProgress) {
        self.events
            .lock()
            .unwrap()
            .push(("content-progress".into(), p.phase.into()));
    }
    fn import_progress(&self, p: &ContentProgress) {
        self.events
            .lock()
            .unwrap()
            .push(("import-progress".into(), p.phase.into()));
    }
    fn content_reloaded(&self, reason: &str) {
        self.events
            .lock()
            .unwrap()
            .push(("content-reloaded".into(), reason.into()));
    }
}

struct Stub(HashMap<String, Vec<u8>>);

impl Fetcher for Stub {
    fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, String> {
        self.0
            .get(url)
            .cloned()
            .ok_or_else(|| "not found".to_string())
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

fn code<T: std::fmt::Debug>(r: Result<T, ApiError>) -> ErrorCode {
    r.expect_err("expected an error").code
}

fn new_hero(app: &AppState, ruleset: Ruleset) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Mira".into(),
        ruleset,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    app.create_character(ruleset, &request).expect("create")
}

fn search_count(app: &AppState, text: &str) -> usize {
    let q: CompendiumQuery =
        serde_json::from_value(json!({ "query": text, "ruleset": "any", "limit": 50 })).unwrap();
    app.search_compendium(&q).expect("search").total
}

fn xml_request() -> ImportPackRequest {
    ImportPackRequest {
        file_name: "test-book.xml".into(),
        content_base64: STANDARD.encode(TEST_XML),
        format: None,
    }
}

fn fixtures() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-import-5etools/tests/fixtures/data")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn manage_commands_round_trip_through_save_and_reopen() {
    let (_dir, app) = app();
    let snap = new_hero(&app, Ruleset::Dnd2014);
    let id = snap.id.clone();
    app.update_details(
        &id,
        &json!({ "eyes": "amber", "backstory": "Raised by lighthouse keepers.", "notesLeft": "L", "alignment": "NG",
                 "playerName": "Ada", "ideal": "Freedom", "sheetAcNote": "+2 behind cover" }),
    )
    .expect("details");
    assert_eq!(
        code(app.update_details(&id, &json!({ "nonsense": "x" }))),
        ErrorCode::InvalidArgument
    );
    let attack = |name: &str| AttackInput {
        name: name.into(),
        damage: "1d6".into(),
        attack_bonus: "+4".into(),
        ability: Some("dex".into()),
        show_in_combat: true,
        ..AttackInput::default()
    };
    app.upsert_attack(&id, &attack("Sling")).expect("attack 1");
    let s = app.upsert_attack(&id, &attack("Dagger")).expect("attack 2");
    let manual: Vec<_> = s.attacks.iter().filter(|a| !a.auto).collect();
    assert_eq!(manual.len(), 2);
    let first = manual[0].id.clone();
    let s = app
        .move_attack(&id, &first, MoveDirection::Down)
        .expect("move");
    let names: Vec<_> = s
        .attacks
        .iter()
        .filter(|a| !a.auto)
        .map(|a| a.name.clone())
        .collect();
    assert_eq!(names, ["Dagger", "Sling"]);
    assert_eq!(
        code(app.remove_attack(&id, "nope")),
        ErrorCode::RequirementFailed
    );
    app.update_companion(
        &id,
        &json!({ "name": "Biscuit", "portrait": "data:image/png;base64,iVBORw0KGgo=" }),
    )
    .expect("companion");
    app.set_character_options(
        &id,
        &OptionsPatch {
            point_buy_unrestricted: Some(true),
            attunement_limit: Some(5),
            ..OptionsPatch::default()
        },
    )
    .expect("options");
    app.save_character(&id).expect("save");
    app.close_character(&id, false).expect("close");

    let back = app.load_character(&id).expect("reopen");
    assert_eq!(back.details.eyes, "amber");
    assert_eq!(back.details.alignment, "NG");
    assert_eq!(back.details.player_name, "Ada");
    assert_eq!(back.details.sheet_ac_note, "+2 behind cover");
    assert_eq!(back.details.backstory, "Raised by lighthouse keepers.");
    let manual: Vec<_> = back.attacks.iter().filter(|a| !a.auto).collect();
    assert_eq!(
        manual.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["Dagger", "Sling"]
    );
    let companion = back.companion.expect("companion");
    assert_eq!(companion.name, "Biscuit");
    assert!(companion.portrait.is_some());
    assert!(back.options.point_buy_unrestricted);
    assert_eq!(back.options.attunement_limit, 5);
    let removed = app.remove_attack(&id, &manual[0].id).expect("remove");
    assert_eq!(removed.attacks.iter().filter(|a| !a.auto).count(), 1);
}

#[test]
fn source_restrictions_preview_and_apply() {
    let (_dir, app) = app();
    let snap = new_hero(&app, Ruleset::Dnd2014);
    let id = snap.id.clone();
    let race_slot = snap
        .slots
        .iter()
        .find(|s| s.title == "Race")
        .unwrap()
        .id
        .clone();
    app.apply_selection(&id, &race_slot, "ID_RACE_HUMAN")
        .expect("race");
    let only_core = vec!["Core".to_string()];
    let preview = app
        .preview_source_restrictions(&id, Some(&only_core))
        .expect("preview");
    assert!(
        preview.affected.iter().any(|a| a.slot_title == "Race"),
        "{preview:?}"
    );
    let applied = app
        .set_source_restrictions(&id, Some(&only_core))
        .expect("apply");
    assert!(applied
        .source_restrictions
        .as_ref()
        .is_some_and(|s| s == &only_core));
    let lifted = app.set_source_restrictions(&id, None).expect("lift");
    assert!(lifted.source_restrictions.is_none());
}

#[test]
fn cross_edition_toggle_and_compatibility_report() {
    let (_dir, app) = app();
    let id = new_hero(&app, Ruleset::Dnd2014).id;
    let strict = app.get_compatibility_report(&id).expect("report");
    assert!(strict.strict, "2014 defaults to strict");
    app.set_allow_cross_edition(&id, Some(true)).expect("allow");
    assert!(!app.get_compatibility_report(&id).unwrap().strict);
    app.set_allow_cross_edition(&id, None).expect("reset");
    assert!(app.get_compatibility_report(&id).unwrap().strict);
    let id24 = new_hero(&app, Ruleset::Dnd2024).id;
    assert!(
        !app.get_compatibility_report(&id24).unwrap().strict,
        "2024 defaults to open"
    );
    app.save_character(&id).unwrap();
    app.set_allow_cross_edition(&id, Some(true)).unwrap();
    app.save_character(&id).unwrap();
    app.close_character(&id, false).unwrap();
    app.load_character(&id).unwrap();
    assert!(
        !app.get_compatibility_report(&id).unwrap().strict,
        "toggle persists"
    );
}

#[test]
fn xml_import_hot_reloads_with_progress_and_can_be_removed() {
    let (dir, app) = app();
    assert_eq!(search_count(&app, "Zephyr Gift"), 0);
    let rec = Recorder::default();
    let result = app.import_pack(&xml_request(), &rec).expect("import");
    assert_eq!(result.report.converted, 1, "{:?}", result.report);
    let pack = result.pack.expect("pack");
    assert_eq!(pack.id, "test-book");
    assert_eq!(pack.kind, PackKind::Imported);
    assert!(pack.enabled);
    assert_eq!(search_count(&app, "Zephyr Gift"), 1);
    assert!(rec.names().contains(&"content-reloaded".to_string()));
    let phases = rec.phases("content-progress");
    assert_eq!(phases.first().map(String::as_str), Some("converting"));
    assert_eq!(phases.last().map(String::as_str), Some("done"));
    assert!(dir.path().join("library/test-book/pack.json").is_file());

    // Survives a restart: a fresh state over the same data directory loads the pack.
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    assert_eq!(
        search_count(&AppState::new(paths).unwrap(), "Zephyr Gift"),
        1
    );

    let packs = app.remove_pack("test-book", &NullSink).expect("remove");
    assert!(packs.iter().all(|p| p.id != "test-book"));
    assert_eq!(search_count(&app, "Zephyr Gift"), 0);
    assert_eq!(
        code(app.remove_pack("srd-5.1", &NullSink)),
        ErrorCode::RequirementFailed
    );
    assert_eq!(
        code(app.remove_pack("missing", &NullSink)),
        ErrorCode::NotFound
    );
}

#[test]
fn bad_import_files_are_rejected_without_changing_content() {
    let (_dir, app) = app();
    let bad = ImportPackRequest {
        file_name: "x.json".into(),
        content_base64: STANDARD.encode("{ not json"),
        format: None,
    };
    assert_eq!(
        code(app.import_pack(&bad, &NullSink)),
        ErrorCode::InvalidArgument
    );
    let notbase64 = ImportPackRequest {
        file_name: "x.xml".into(),
        content_base64: "***".into(),
        format: None,
    };
    assert_eq!(
        code(app.import_pack(&notbase64, &NullSink)),
        ErrorCode::InvalidArgument
    );
    assert_eq!(app.list_packs().unwrap().len(), 5);
}

#[test]
fn pack_disable_and_enable_changes_available_content() {
    let (_dir, app) = app();
    app.import_pack(&xml_request(), &NullSink).expect("import");
    let races_2014 = |app: &AppState, id: &str, slot: &str| {
        app.get_slot_options(id, slot, None, Some(200), None)
            .expect("options")
            .total
    };
    let snap = new_hero(&app, Ruleset::Dnd2014);
    let race_slot = snap
        .slots
        .iter()
        .find(|s| s.title == "Race")
        .unwrap()
        .id
        .clone();
    let before = races_2014(&app, &snap.id, &race_slot);

    let rec = Recorder::default();
    let packs = app
        .set_pack_enabled("srd-5.1", false, &rec)
        .expect("disable");
    assert!(packs
        .iter()
        .any(|p| p.id == "srd-5.1" && !p.enabled && p.element_count > 0));
    assert!(rec.names().contains(&"content-reloaded".to_string()));
    let after = races_2014(&app, &snap.id, &race_slot);
    assert!(
        after < before,
        "disabling srd-5.1 removes races: {after} < {before}"
    );
    assert!(app
        .list_sources()
        .unwrap()
        .iter()
        .all(|s| s.pack_id != "srd-5.1"));

    app.set_pack_enabled("srd-5.1", true, &NullSink)
        .expect("enable");
    assert_eq!(races_2014(&app, &snap.id, &race_slot), before);

    app.set_pack_enabled("test-book", false, &NullSink)
        .expect("disable user pack");
    assert_eq!(search_count(&app, "Zephyr Gift"), 0);
    let listed = app.list_packs().unwrap();
    assert!(listed
        .iter()
        .any(|p| p.id == "test-book" && !p.enabled && p.element_count == 1));
    app.set_pack_enabled("test-book", true, &NullSink)
        .expect("enable user pack");
    assert_eq!(search_count(&app, "Zephyr Gift"), 1);
    assert_eq!(
        code(app.set_pack_enabled("core", false, &NullSink)),
        ErrorCode::RequirementFailed
    );
}

#[test]
fn five_etools_scan_and_import_use_the_importer_fixtures() {
    let (dir, app) = app();
    // Raw paths, unknown tokens and tokens minted for another purpose are all refused.
    assert_eq!(
        code(app.scan_5etools_folder(&fixtures())),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.scan_5etools_folder("pick_unknown")),
        ErrorCode::InvalidArgument
    );
    let wrong = app
        .picked
        .grant(fixtures().into(), PickPurpose::ImportFile)
        .unwrap();
    assert_eq!(
        code(app.scan_5etools_folder(&wrong.token)),
        ErrorCode::InvalidArgument
    );
    let missing = app
        .picked
        .grant("/definitely/not/here".into(), PickPurpose::FivetoolsFolder)
        .unwrap();
    assert_eq!(
        code(app.scan_5etools_folder(&missing.token)),
        ErrorCode::InvalidArgument
    );
    let folder = app
        .picked
        .grant(fixtures().into(), PickPurpose::FivetoolsFolder)
        .unwrap()
        .token;
    let scan = app.scan_5etools_folder(&folder).expect("scan");
    assert!(scan
        .sources
        .iter()
        .any(|s| s.abbr == "PHB" && s.ruleset == "dnd2014"));
    assert_eq!(
        code(app.import_5etools(
            &FiveToolsImportRequest {
                folder: wrong.token.clone(),
                sources: vec!["PHB".into()],
                include_fluff: false,
            },
            &NullSink
        )),
        ErrorCode::InvalidArgument
    );
    let request = FiveToolsImportRequest {
        folder,
        sources: vec!["PHB".into()],
        include_fluff: false,
    };
    let rec = Recorder::default();
    let report = app.import_5etools(&request, &rec).expect("import");
    assert!(report.converted > 0);
    assert_eq!(report.packs.len(), 1);
    let pack = &report.packs[0];
    assert_eq!(pack.kind, PackKind::LocalImport);
    assert_eq!(pack.redistributable, Some(false));
    assert!(pack.enabled && pack.element_count > 0);
    let phases = rec.phases("import-progress");
    assert!(phases.contains(&"converting".to_string()));
    assert_eq!(phases.last().map(String::as_str), Some("done"));
    assert!(rec.names().contains(&"content-reloaded".to_string()));
    assert!(dir.path().join("library/5etools-phb/import.json").is_file());
    let listed = app.list_packs().unwrap();
    assert!(listed
        .iter()
        .any(|p| p.id == pack.id && p.redistributable == Some(false)));
    assert_eq!(
        code(app.import_5etools(
            &FiveToolsImportRequest {
                sources: vec![],
                ..request
            },
            &NullSink
        )),
        ErrorCode::InvalidArgument
    );
}

fn index_json(hash: Option<&str>) -> Vec<u8> {
    let attributes = hash.map_or_else(|| json!({}), |h| json!({ "sha256": h }));
    serde_json::to_vec(&json!({
        "format": "borealis-index/1",
        "info": { "name": "Test Index" },
        "files": [{
            "name": "test-book.xml",
            "url": "https://example.invalid/test-book.xml",
            "extra": { "attributes": attributes }
        }]
    }))
    .unwrap()
}

const INDEX_URL: &str = "https://example.invalid/index.json";
const FILE_URL: &str = "https://example.invalid/test-book.xml";

fn stub(hash: Option<&str>) -> Stub {
    Stub(HashMap::from([
        (INDEX_URL.to_string(), index_json(hash)),
        (FILE_URL.to_string(), TEST_XML.as_bytes().to_vec()),
    ]))
}

#[test]
fn no_default_indexes_and_only_https_urls_are_accepted() {
    let (_dir, app) = app();
    assert!(app.list_indexes().is_empty());
    assert_eq!(
        code(app.add_index("http://example.invalid/i.json")),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.add_index("file:///etc/passwd")),
        ErrorCode::InvalidArgument
    );
    let added = app.add_index(INDEX_URL).expect("add");
    assert_eq!(added.len(), 1);
    assert_eq!(app.add_index(INDEX_URL).unwrap().len(), 1, "no duplicates");
    assert!(app.remove_index(INDEX_URL).unwrap().is_empty());
}

#[test]
fn index_update_installs_verified_files_and_reloads() {
    let (_dir, app) = app();
    app.add_index(INDEX_URL).unwrap();
    let hash = sha256_hex(TEST_XML.as_bytes());
    let rec = Recorder::default();
    let result = app
        .update_indexes(&stub(Some(&hash)), &rec)
        .expect("update");
    assert_eq!(
        (result.updated, result.failed.len(), result.reloaded),
        (1, 0, true)
    );
    assert_eq!(search_count(&app, "Zephyr Gift"), 1);
    let pack = app
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.kind == PackKind::Downloaded)
        .expect("downloaded pack");
    assert!(pack.id.starts_with("dl-test-index-"));
    let phases = rec.phases("content-progress");
    for wanted in ["checking", "downloading", "reloading", "done"] {
        assert!(
            phases.contains(&wanted.to_string()),
            "{wanted} in {phases:?}"
        );
    }
    let index = &app.list_indexes()[0];
    assert_eq!(
        (
            index.name.as_str(),
            index.file_count,
            index.updates_available
        ),
        ("Test Index", 1, 0)
    );
    assert!(index.last_checked.is_some());
    // Second run: the hash matches what is installed, so nothing is downloaded or reloaded.
    let again = app.update_indexes(&stub(Some(&hash)), &NullSink).unwrap();
    assert_eq!((again.updated, again.reloaded), (0, false));
    // Clearing downloads removes the pack.
    let packs = app.clear_downloaded_content(&NullSink).unwrap();
    assert!(packs.iter().all(|p| p.kind != PackKind::Downloaded));
    assert_eq!(search_count(&app, "Zephyr Gift"), 0);
}

#[test]
fn index_update_rejects_a_hash_mismatch() {
    let (_dir, app) = app();
    app.add_index(INDEX_URL).unwrap();
    let wrong = sha256_hex(b"something else");
    let result = app
        .update_indexes(&stub(Some(&wrong)), &NullSink)
        .expect("update");
    assert_eq!((result.updated, result.reloaded), (0, false));
    assert_eq!(result.failed, [FILE_URL]);
    assert_eq!(search_count(&app, "Zephyr Gift"), 0);
    assert!(app
        .list_packs()
        .unwrap()
        .iter()
        .all(|p| p.kind != PackKind::Downloaded));
    assert_eq!(app.list_indexes()[0].updates_available, 1);
}

#[test]
fn index_update_refuses_entries_without_a_hash_and_unreachable_indexes_fail() {
    let (_dir, app) = app();
    app.add_index(INDEX_URL).unwrap();
    app.add_index("https://example.invalid/missing.json")
        .unwrap();
    let result = app.update_indexes(&stub(None), &NullSink).expect("update");
    assert_eq!(result.updated, 0);
    assert_eq!(
        result.failed,
        [FILE_URL, "https://example.invalid/missing.json"]
    );
    assert_eq!(search_count(&app, "Zephyr Gift"), 0);
}
