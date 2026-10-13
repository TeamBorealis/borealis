//! A character built on homebrew whose legacy id Borealis' id rules reject must resolve
//! to the migrator's normalised element (synthetic content and character, no fixtures).

use borealis_character::import_dnd5e;
use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin};
use borealis_engine::Engine;
use borealis_migrate::{migrate_path, MigrateOptions};

const CONTENT: &str = r#"<elements>
  <element name="Odd" type="Option" source="Hb" id="ID_HB_OPTION_O'ODD_(X)"/>
</elements>"#;

fn character(id: &str) -> String {
    format!(
        r#"<character version="1.19.309"><build><input><name>T</name></input><elements>
<element type="Option" name="Odd" id="{id}" />
</elements></build></character>"#
    )
}

#[test]
fn homebrew_option_with_unsafe_id_resolves_to_the_normalised_element() {
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
    let new_id = store.element("ID_HB_OPTION_O'ODD_(X)").unwrap().id.clone();
    assert_eq!(new_id, "ID_HB_OPTION_O_ODD__X_");

    let imported = import_dnd5e(&engine, &character("ID_HB_OPTION_O'ODD_(X)")).unwrap();
    assert_eq!(imported.character.build.options, vec![new_id]);
    assert!(imported.report.missing_content.is_empty());
}
