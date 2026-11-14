//! Sources page flags: a book's `Source` element belongs to the `Core` source (as in Aurora content), so
//! `list_sources` finds it by the book's name; the restrict toggles persist in the settings file.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::ImportPackRequest;
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, Ruleset};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::PathBuf;

const BOOKS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>Flag Books</name>
    <update version="1.0.0"><file name="flag-books.xml" url="https://example.invalid/flag-books.xml"/></update>
  </info>
  <element name="Playtest Pages" type="Source" source="Core" id="ID_TEST_SOURCE_PLAYTEST">
    <description><p>Playtest material for the tests.</p></description>
    <setters><set name="url"></set><set name="abbreviation">TB</set><set name="official">true</set><set name="playtest">true</set></setters>
  </element>
  <element name="Friendly Press Guide" type="Source" source="Core" id="ID_TEST_SOURCE_THIRD">
    <description><p>A third-party book.</p></description>
    <setters><set name="url"></set><set name="abbreviation">TB</set><set name="author">Friendly Press</set></setters>
  </element>
  <element name="Official Book" type="Source" source="Core" id="ID_TEST_SOURCE_OFFICIAL">
    <setters><set name="url"></set><set name="abbreviation">TB</set><set name="official">true</set></setters>
  </element>
  <element name="Draftling" type="Race" source="Playtest Pages" id="ID_TEST_RACE_DRAFTLING">
    <description><p>A playtest folk.</p></description>
    <sheet display="false" />
    <rules>
      <grant type="Size" id="ID_SIZE_MEDIUM" />
      <stat name="innate speed" value="30" bonus="base" />
    </rules>
  </element>
  <element name="Draft Gift" type="Feat" source="Playtest Pages" id="ID_TEST_FEAT_DRAFT">
    <description><p>A playtest feat.</p></description>
    <sheet display="true"><description>Test feat.</description></sheet>
    <setters><set name="category">General</set></setters>
  </element>
  <element name="Press Gift" type="Feat" source="Friendly Press Guide" id="ID_TEST_FEAT_PRESS">
    <description><p>A third-party feat.</p></description>
    <sheet display="true"><description>Test feat.</description></sheet>
    <setters><set name="category">General</set></setters>
  </element>
  <element name="Plain Gift" type="Feat" source="Official Book" id="ID_TEST_FEAT_PLAIN">
    <description><p>An official feat.</p></description>
    <sheet display="true"><description>Test feat.</description></sheet>
    <setters><set name="category">General</set></setters>
  </element>
</elements>"#;

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn import_books(app: &AppState) {
    app.import_pack(
        &ImportPackRequest {
            file_name: "flag-books.xml".into(),
            content_base64: STANDARD.encode(BOOKS_XML),
            format: None,
        },
        &NullSink,
    )
    .expect("import");
}

#[test]
fn a_core_source_element_flags_its_book() {
    let (_dir, app) = app();
    import_books(&app);
    let sources = app.list_sources().expect("sources");
    let find = |name: &str| {
        sources.iter().find(|s| s.id == name).unwrap_or_else(|| {
            panic!(
                "{name} not in {:?}",
                sources.iter().map(|s| &s.id).collect::<Vec<_>>()
            )
        })
    };
    let playtest = find("Playtest Pages");
    assert!(playtest.playtest && !playtest.third_party);
    assert_eq!(playtest.description, "Playtest material for the tests.");
    let third = find("Friendly Press Guide");
    assert!(third.third_party && !third.playtest);
    let official = find("Official Book");
    assert!(!official.third_party && !official.playtest);
    // The bundled SRD books describe themselves and are official.
    assert!(sources
        .iter()
        .filter(|s| s.pack_id.starts_with("srd-"))
        .all(|s| !s.playtest && !s.third_party));
}

#[test]
fn the_restrict_toggles_persist_in_the_settings_file() {
    let (dir, app) = app();
    let saved = app
        .save_settings(&json!({ "restrictPlaytest": true, "restrictThirdParty": true }))
        .expect("save");
    assert!(saved.restrict_playtest && saved.restrict_third_party);
    let reopened = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    })
    .expect("state");
    let s = reopened.settings().expect("settings");
    assert!(s.restrict_playtest && s.restrict_third_party);
    assert!(
        !app.restore_default_settings(&NullSink)
            .expect("defaults")
            .restrict_playtest
    );
}

/// What the "Restrict playtest content" toggle sends for a loaded character: every listed book but the playtest ones.
/// A pick from a playtest book stays on the character (and survives a reload); only new choices leave it out.
#[test]
fn restricting_playtest_keeps_an_existing_pick() {
    let (_dir, app) = app();
    import_books(&app);
    let request = NewCharacterRequest {
        name: "Mira".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let snap = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    let race_slot = |s: &borealis_lib::dto::CharacterSnapshot| {
        s.slots
            .iter()
            .find(|s| s.title == "Race")
            .expect("race slot")
            .clone()
    };
    let slot_id = race_slot(&snap).id;
    app.apply_selection(&snap.id, &slot_id, "ID_TEST_RACE_DRAFTLING")
        .expect("pick the playtest race");

    let keep: Vec<String> = app
        .list_sources()
        .expect("sources")
        .into_iter()
        .filter(|s| !s.playtest)
        .map(|s| s.id)
        .collect();
    assert!(!keep.iter().any(|id| id == "Playtest Pages"));
    let preview = app
        .preview_source_restrictions(&snap.id, Some(&keep))
        .expect("preview");
    assert!(
        preview.affected.iter().any(|a| a.slot_title == "Race"),
        "{preview:?}"
    );
    app.set_source_restrictions(&snap.id, Some(&keep))
        .expect("apply");

    let reloaded = app.load_character(&snap.id).expect("reload");
    let chosen = race_slot(&reloaded).chosen.map(|c| c.id);
    assert_eq!(chosen.as_deref(), Some("ID_TEST_RACE_DRAFTLING"));
    // A new character on the same restriction is not offered the playtest race.
    let fresh = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    app.set_source_restrictions(&fresh.id, Some(&keep))
        .expect("restrict");
    let offered = app
        .get_slot_options(&fresh.id, &race_slot(&fresh).id, None, Some(500), None)
        .expect("options");
    assert!(offered.total > 0, "other races stay offered");
    assert!(
        !offered
            .items
            .iter()
            .any(|o| o.base.summary.id == "ID_TEST_RACE_DRAFTLING"),
        "the playtest race is not offered as a new choice"
    );
}
