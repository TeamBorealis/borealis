//! Content may declare two selects nothing tells apart (same name, type, level and supports;
//! a community archetype does this for its level-3 spells). An Aurora save then stores both
//! choices with ordinal 1 and the same checksum; each must land in its own slot instead of the
//! second overwriting the first (synthetic content and character, no fixtures).

use borealis_character::import_dnd5e;
use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin};
use borealis_engine::state::Choice;
use borealis_engine::Engine;
use borealis_migrate::{migrate_path, MigrateOptions};

const CONTENT: &str = r#"<elements>
  <element name="Twin" type="Option" source="Hb" id="ID_HB_OPTION_TWIN">
    <rules>
      <select type="Language" name="Tongue" supports="Hb Tongue" />
      <select type="Language" name="Tongue" supports="Hb Tongue" />
    </rules>
  </element>
  <element name="A" type="Language" source="Hb" id="ID_HB_LANGUAGE_A"><supports>Hb Tongue</supports></element>
  <element name="B" type="Language" source="Hb" id="ID_HB_LANGUAGE_B"><supports>Hb Tongue</supports></element>
</elements>"#;

const CHARACTER: &str = r#"<character version="1.19.309"><build><input><name>T</name></input><elements>
<element type="Option" name="Twin" id="ID_HB_OPTION_TWIN">
  <element type="Language" name="Tongue" requiredLevel="1" checksum="0" registered="ID_HB_LANGUAGE_A" />
  <element type="Language" name="Tongue" requiredLevel="1" checksum="0" registered="ID_HB_LANGUAGE_B" />
</element>
</elements></build></character>"#;

#[test]
fn choices_of_two_identical_selects_both_survive_the_import() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.xml"), CONTENT).unwrap();
    let out = migrate_path(tmp.path(), &MigrateOptions::default()).unwrap();
    let mut b = ContentStoreBuilder::new();
    for f in out.files.iter().filter(|f| f.path == "a.json") {
        b = b.add_file(
            "a.json",
            PackOrigin::Custom,
            ContentFile::from_json_str(&f.contents).unwrap(),
        );
    }
    let store = b.build();
    let engine = Engine::new(&store);

    let imported = import_dnd5e(&engine, CHARACTER).unwrap();
    let mut picks: Vec<(u32, String)> = imported
        .character
        .build
        .selections
        .iter()
        .filter(|s| s.slot.owner == "ID_HB_OPTION_TWIN")
        .filter_map(|s| match &s.choice {
            Choice::Element(id) => Some((s.slot.rule, id.clone())),
            Choice::ListItem(_) => None,
        })
        .collect();
    picks.sort();
    assert_eq!(picks.len(), 2, "{picks:?}");
    assert_ne!(
        picks[0].0, picks[1].0,
        "both choices in one slot: {picks:?}"
    );
    let mut ids: Vec<&str> = picks.iter().map(|(_, id)| id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["ID_HB_LANGUAGE_A", "ID_HB_LANGUAGE_B"]);
    assert_eq!(imported.report.selections_imported, 2);
}
