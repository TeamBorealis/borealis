//! Re-resolving an imported character after the missing content is installed later.
//! Fully synthetic: a tiny pack is converted from XML on top of the core pack.

use borealis_character::{import_dnd5e, recheck, source_hint, Character};
use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin};
use borealis_engine::state::Choice;
use borealis_engine::Engine;
use borealis_migrate::{migrate_path, MigrateOptions};
use std::path::Path;

const PACK: &str = r#"<elements>
  <info><name>Synthetic</name><update version="0.0.1"><file name="synthetic.xml" url="https://example.invalid/synthetic.xml"/></update></info>
  <element name="Warden" type="Class" source="Synthetic" id="ID_ZZ_SYN_CLASS_WARDEN">
    <supports>Warden</supports>
    <setters><set name="hd">d8</set></setters>
    <rules><select type="Archetype" name="Warden Path" supports="Warden Path" level="1"/></rules>
  </element>
  <element name="Path of Oaks" type="Archetype" source="Synthetic" id="ID_ZZ_SYN_ARCHETYPE_OAKS">
    <supports>Warden Path</supports>
    <rules><select type="Archetype Feature" name="Oak Gift" supports="Oak Gift" level="1"/></rules>
  </element>
  <element name="Bark Skin" type="Archetype Feature" source="Synthetic" id="ID_ZZ_SYN_FEATURE_BARK">
    <supports>Oak Gift</supports>
  </element>
  <element name="Acorn Charm" type="Option" source="Synthetic" id="ID_ZZ_SYN_OPTION_ACORN"/>
</elements>"#;

const CHARACTER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<character version="1.0.0">
  <information><name>Synthetic Hero</name></information>
  <build>
    <input/>
    <elements>
      <element type="Option" name="Acorn Charm" id="ID_ZZ_SYN_OPTION_ACORN"/>
      <element type="Level" name="Level 1" id="ID_ZZ_SYN_CLASS_WARDEN" class="ID_ZZ_SYN_CLASS_WARDEN">
        <element type="Archetype" name="Warden Path" registered="ID_ZZ_SYN_ARCHETYPE_OAKS" number="1" requiredLevel="1">
          <element type="Archetype Feature" name="Oak Gift" registered="ID_ZZ_SYN_FEATURE_BARK" number="1" requiredLevel="1"/>
        </element>
      </element>
    </elements>
    <sum><element id="ID_ZZ_SYN_ARCHETYPE_OAKS" type="Archetype" name="Path of Oaks"/></sum>
  </build>
</character>"#;

fn engine(with_pack: bool) -> Engine {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let mut b = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core");
    if with_pack {
        let dir = std::env::temp_dir().join(format!("borealis-recheck-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp");
        let input = dir.join("syn.xml");
        std::fs::write(&input, PACK).expect("write");
        let opts = MigrateOptions {
            origin: PackOrigin::Custom,
            validate: false,
            ..MigrateOptions::default()
        };
        let out = migrate_path(&input, &opts).expect("migrate");
        for f in out.files.iter().filter(|f| f.path.ends_with(".json")) {
            if let Ok(cf) = ContentFile::from_json_str(&f.contents) {
                b = b.add_file(f.path.clone(), PackOrigin::Custom, cf);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    Engine::new(&b.build())
}

#[test]
fn import_without_the_pack_records_what_is_missing() {
    let e = engine(false);
    let out = import_dnd5e(&e, CHARACTER).expect("import");
    let ids: Vec<&str> = out
        .report
        .missing_content
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert!(ids.contains(&"ID_ZZ_SYN_OPTION_ACORN"), "{ids:?}");
    assert!(ids.contains(&"ID_ZZ_SYN_CLASS_WARDEN"), "{ids:?}");
    assert_eq!(out.report.unmapped_slots.len(), 2);
    assert_eq!(out.character.unresolved.len(), 2);
    let info = out.character.import.as_ref().expect("info");
    assert_eq!(info.missing.len(), ids.len());
    assert!(out.character.build.options.is_empty());
}

#[test]
fn recheck_picks_up_content_installed_later_and_persists() {
    let before = engine(false);
    let mut ch = import_dnd5e(&before, CHARACTER).expect("import").character;
    // Nothing changes while the content is still absent.
    let idle = recheck(&before, &mut ch);
    assert_eq!(idle.resolved, 0);
    assert!(!idle.changed());
    assert_eq!(idle.still_unmapped.len(), 2);

    let after = engine(true);
    let out = recheck(&after, &mut ch);
    assert!(out.changed());
    assert!(out.still_unmapped.is_empty(), "{:?}", out.still_unmapped);
    assert!(ch.unresolved.is_empty());
    assert!(ch
        .build
        .options
        .contains(&"ID_ZZ_SYN_OPTION_ACORN".to_string()));
    let chosen: Vec<&Choice> = ch.build.selections.iter().map(|s| &s.choice).collect();
    assert!(chosen.contains(&&Choice::Element("ID_ZZ_SYN_ARCHETYPE_OAKS".into())));
    assert!(chosen.contains(&&Choice::Element("ID_ZZ_SYN_FEATURE_BARK".into())));
    assert!(out.still_missing.is_empty(), "{:?}", out.still_missing);
    assert_eq!(ch.import.as_ref().expect("info").missing_content, 0);

    // Persisted: the saved file round-trips and a second pass has nothing left to do.
    let json = ch.to_json_string().expect("json");
    let mut again = Character::from_json_str(&json).expect("load");
    assert_eq!(again, ch);
    assert_eq!(recheck(&after, &mut again).resolved, 0);
}

#[test]
fn unresolved_records_survive_save_and_load() {
    let e = engine(false);
    let ch = import_dnd5e(&e, CHARACTER).expect("import").character;
    let back = Character::from_json_str(&ch.to_json_string().expect("json")).expect("load");
    assert_eq!(back.unresolved, ch.unresolved);
    assert_eq!(back.unresolved[0].required_level, Some(1));
    assert_eq!(back.import, ch.import);
    assert!(!back.import.expect("info").missing.is_empty());
}

#[test]
fn source_hint_names_the_book() {
    assert_eq!(source_hint("ID_WOTC_PHB_RACE_HUMAN"), "WOTC PHB");
    assert_eq!(source_hint("ID_PHB_CLASS_FEATURE_X"), "PHB");
    assert_eq!(source_hint("custom"), "");
}

#[test]
fn internal_ids_the_engine_lacks_are_reported_and_types_come_from_sum() {
    let xml = CHARACTER.replace(
        "<sum>",
        r#"<equipment><item id="ID_ZZ_INTERNAL_ITEM_PROXY" name="Proxy" amount="1"/></equipment><sum><element id="ID_ZZ_INTERNAL_GRANT_X" type="Grants" name="x"/><element id="ID_ZZ_SYN_CLASS_WARDEN" type="Class" name="Warden"/>"#,
    );
    let e = engine(false);
    let out = import_dnd5e(&e, &xml).expect("import");
    let miss = &out.report.missing_content;
    // an Aurora internal id Borealis does not define is lost content, never dropped silently
    // (det-char-1; it was filtered out before)
    for id in ["ID_ZZ_INTERNAL_ITEM_PROXY", "ID_ZZ_INTERNAL_GRANT_X"] {
        assert!(miss.iter().any(|m| m.id == id), "{id}: {miss:?}");
    }
    let owner = miss
        .iter()
        .find(|m| m.id == "ID_ZZ_SYN_CLASS_WARDEN")
        .expect("owner");
    assert_eq!(owner.ty, "Class");
}
