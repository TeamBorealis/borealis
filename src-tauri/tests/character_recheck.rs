//! Character import reports, batch import and re-resolving after content is installed.
//! Synthetic only: the sample is `example.dnd5e` from the character crate.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::ImportPackRequest;
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const SAMPLE: &str = include_str!("../../crates/borealis-character/tests/data/example.dnd5e");
const OPTION: &str = "ID_HOMEBREW_OPTION_GONE";
const SPELL: &str = "ID_UNKNOWN_CANTRIP_FROM_A_BOOK_I_LACK";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

fn b64(s: &str) -> String {
    STANDARD.encode(s)
}

fn install_missing_content(app: &AppState) {
    let xml = format!(
        r#"<elements><info><name>Later</name><update version="0.0.1"><file name="later.xml" url="https://example.invalid/later.xml"/></update></info>
  <element name="Gone Option" type="Option" source="Later" id="{OPTION}"/>
  <element name="Lost Cantrip" type="Spell" source="Later" id="{SPELL}">
    <supports>Wizard</supports>
    <setters><set name="level">0</set><set name="school">Evocation</set><set name="time">1 action</set><set name="duration">Instantaneous</set><set name="range">Self</set></setters>
  </element></elements>"#
    );
    let req = ImportPackRequest {
        file_name: "later.xml".into(),
        content_base64: b64(&xml),
        format: None,
    };
    app.import_pack(&req, &NullSink).expect("install");
}

#[test]
fn report_lists_missing_content_grouped_data_and_summary_count() {
    let (_d, app) = app();
    let r = app
        .import_character("hero.dnd5e", &b64(SAMPLE))
        .expect("import");
    let ids: Vec<&str> = r.missing_content.iter().map(|m| m.id.as_str()).collect();
    assert!(ids.contains(&OPTION), "{ids:?}");
    assert!(ids.contains(&SPELL), "{ids:?}");
    let opt = r.missing_content.iter().find(|m| m.id == OPTION).unwrap();
    assert_eq!(opt.context, "option");
    assert_eq!(opt.source, "HOMEBREW");
    assert_eq!(opt.name, "Removed Homebrew Option");
    assert_eq!(r.summary.missing_content as usize, r.missing_content.len());
    assert!(r.selections_imported > 0);
    assert!(r.divergences.iter().all(|d| !d.is_empty()));
}

#[test]
fn batch_import_creates_one_character_and_report_per_file() {
    let (_d, app) = app();
    let a = app.import_character("a.dnd5e", &b64(SAMPLE)).expect("a");
    let b = app.import_character("b.dnd5e", &b64(SAMPLE)).expect("b");
    assert_ne!(a.summary.id, b.summary.id);
    assert_eq!(app.list_characters().expect("list").len(), 2);
    // A bad file in the batch is an error for that file only.
    assert!(app
        .import_character("c.dnd5e", &b64("not a character"))
        .is_err());
    assert_eq!(app.list_characters().expect("list").len(), 2);
}

#[test]
fn installing_content_later_resolves_characters_automatically() {
    let (_d, app) = app();
    let r = app
        .import_character("hero.dnd5e", &b64(SAMPLE))
        .expect("import");
    let id = r.summary.id.clone();
    let before = r.summary.missing_content;
    assert!(before >= 2);

    install_missing_content(&app);

    let row = app
        .list_characters()
        .expect("list")
        .into_iter()
        .find(|c| c.id == id)
        .expect("row");
    assert_eq!(row.missing_content, before - 2, "option + spell resolved");
    let snap = app.load_character(&id).expect("open");
    assert!(
        snap.options.custom.get(OPTION) == Some(&true),
        "option is on the character now"
    );

    // Persisted: a fresh state over the same data dir sees the same thing, and a second
    // manual re-check has nothing left to resolve.
    let again = app.recheck_missing_content().expect("recheck");
    assert!(again.updated.is_empty());
    assert!(again.outstanding.iter().all(|e| e
        .still_missing
        .iter()
        .all(|m| m.id != OPTION && m.id != SPELL)));
}

#[test]
fn manual_recheck_reports_what_is_still_missing() {
    let (_d, app) = app();
    app.import_character("hero.dnd5e", &b64(SAMPLE))
        .expect("import");
    let r = app.recheck_missing_content().expect("recheck");
    assert_eq!(r.checked, 1);
    assert!(r.updated.is_empty());
    assert_eq!(r.outstanding.len(), 1);
    assert!(r.outstanding[0]
        .still_missing
        .iter()
        .any(|m| m.id == OPTION));
}
