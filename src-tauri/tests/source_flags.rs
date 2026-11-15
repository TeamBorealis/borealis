//! Sources page flags: a book's `Source` element belongs to the `Core` source (as in XML content packs), so
//! `list_sources` finds it by the book's name, and its `playtest` flag, description and note reach the page.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::ImportPackRequest;
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const BOOKS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>Flag Books</name>
    <update version="1.0.0"><file name="flag-books.xml" url="https://example.invalid/flag-books.xml"/></update>
  </info>
  <element name="Playtest Pages" type="Source" source="Core" id="ID_TEST_SOURCE_PLAYTEST">
    <description><p>Playtest material for the tests.</p></description>
    <setters>
      <set name="url"></set><set name="abbreviation">TP</set><set name="official">true</set>
      <set name="playtest">true</set><set name="information">Subject to change.</set>
    </setters>
  </element>
  <element name="Finished Book" type="Source" source="Core" id="ID_TEST_SOURCE_FINISHED">
    <description><p>A finished book.</p></description>
    <setters><set name="url"></set><set name="abbreviation">TF</set><set name="official">true</set></setters>
  </element>
  <element name="Draft Gift" type="Feat" source="Playtest Pages" id="ID_TEST_FEAT_DRAFT">
    <description><p>A playtest feat.</p></description>
    <sheet display="true"><description>Test feat.</description></sheet>
    <setters><set name="category">General</set></setters>
  </element>
  <element name="Plain Gift" type="Feat" source="Finished Book" id="ID_TEST_FEAT_PLAIN">
    <description><p>A finished feat.</p></description>
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

#[test]
fn a_core_source_element_describes_its_book() {
    let (_dir, app) = app();
    app.import_pack(
        &ImportPackRequest {
            file_name: "flag-books.xml".into(),
            content_base64: STANDARD.encode(BOOKS_XML),
            format: None,
        },
        &NullSink,
    )
    .expect("import");
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
    assert!(playtest.playtest, "the flask button finds playtest books");
    assert_eq!(playtest.description, "Playtest material for the tests.");
    assert_eq!(playtest.info, "Subject to change.");
    let finished = find("Finished Book");
    assert!(!finished.playtest);
    assert_eq!(finished.description, "A finished book.");
    // The bundled SRD books, whose Source element names its own book, are unchanged.
    assert!(sources
        .iter()
        .filter(|s| s.pack_id.starts_with("srd-"))
        .all(|s| !s.playtest && !s.description.is_empty()));
}
