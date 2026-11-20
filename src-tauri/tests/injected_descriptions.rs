//! GitHub #53: an Aurora-format option whose description is only a reference to another
//! element (`<div element="ID"/>`, as a draconic ancestry option points at its ancestry trait)
//! shows that element's text in the builder's description pane, as Aurora's description panel
//! does (ReplaceInjectedElementDescription): the upper-cased name as a heading, then the text.

use borealis_lib::content_dto::ImportPackRequest;
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const ANCESTRY: &str = "ID_T_INJ_RACIAL_TRAIT_ANCESTRY";
const GOLD: &str = "ID_T_INJ_RACIAL_TRAIT_ANCESTRY_GOLD";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn install(app: &AppState) {
    let xml = format!(
        r#"<elements><info><name>Injection Test</name><update version="0.0.1"><file name="injection.xml" url="https://example.invalid/injection.xml"/></update></info>
<element name="Test Ancestry" type="Racial Trait" source="Injection Test" id="{ANCESTRY}">
  <description><p>You descend from a test dragon. Gold dragons breathe fire.</p></description>
  <rules><select type="Racial Trait" name="Test Ancestry" supports="Test Ancestry" /></rules>
</element>
<element name="Gold" type="Racial Trait" source="Injection Test" id="{GOLD}">
  <supports>Test Ancestry</supports>
  <compendium display="false" />
  <description>
    <div element="{ANCESTRY}" />
  </description>
  <rules><stat name="test ancestry:damage type" value="Fire" inline="true" /></rules>
</element>
</elements>"#
    );
    use base64::Engine as _;
    let req = ImportPackRequest {
        file_name: "injection.xml".into(),
        content_base64: base64::engine::general_purpose::STANDARD.encode(xml),
        format: None,
    };
    app.import_pack(&req, &NullSink).expect("install");
}

#[test]
fn an_option_described_by_a_reference_shows_the_referenced_text() {
    let (_d, app) = app();
    install(&app);
    let gold = app.get_element(GOLD).expect("Gold");
    let html = &gold.description_html;
    assert!(
        html.contains("<h5>TEST ANCESTRY</h5>"),
        "the referenced element's name heads the text: {html}"
    );
    assert!(
        html.contains("You descend from a test dragon. Gold dragons breathe fire."),
        "the referenced element's description follows: {html}"
    );
    assert!(
        gold.links.iter().all(|l| l.summary.id != ANCESTRY),
        "the injected element is not also a linked card: {:?}",
        gold.links
            .iter()
            .map(|l| (&l.summary.id, &l.via))
            .collect::<Vec<_>>()
    );
}
