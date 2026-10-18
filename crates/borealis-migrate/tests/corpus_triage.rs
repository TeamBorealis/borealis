//! Synthetic regression for the corpus-sweep "store errors" triage.
//! Every class of error the sweep reported is a content
//! problem, not a migrator or loader bug: the migrator must keep converting, surface a clear
//! per-element diagnostic, and the store must still load and stay usable.

use borealis_content::{ContentStoreBuilder, IssueCode, PackOrigin, Severity};
use borealis_migrate::{migrate_path, write_output, MigrateOptions};

const BROKEN_HOMEBREW: &str = r#"<elements>
  <element name="Apostrophe" type="Feat" source="Hb" id="ID_HB_FEAT_APOS'S_ID"><description><p>a</p></description></element>
  <element name="Pipe" type="Feat" source="Hb" id="ID_HB_FEAT_PIPE|_CHAR"><description><p>b</p></description></element>
  <element name="Paren" type="Feat" source="Hb" id="ID_HB_FEAT_ARCANE_PROF(I)"><description><p>c</p></description></element>
  <element name="Curly" type="Feat" source="Hb" id="ID_HB_FEAT_CURLY’S_ID"><description><p>d</p></description></element>
  <element name="Fine" type="Feat" source="Hb" id="ID_HB_FEAT_FINE"><description><p>e</p></description></element>
</elements>"#;

// A system-stub file: empty shells that shadow fully defined elements of the same id.
const STUBS: &str = r#"<elements>
  <element name="Real" type="Level" source="Hb" id="ID_HB_LEVEL_1"><setters><set name="Level">1</set><set name="Experience">0</set></setters></element>
  <element name="Real" type="Level" source="Hb" id="ID_HB_LEVEL_1" />
  <element name="Wizard" type="Class" source="Hb" id="ID_HB_CLASS_X"><setters><set name="hd">d6</set></setters><multiclass id="ID_HB_MULTICLASS_X"/></element>
  <element name="Wizard" type="Multiclass" source="Hb" id="ID_HB_MULTICLASS_X" />
</elements>"#;

fn migrate_and_load(
    xml: &str,
) -> (
    borealis_migrate::MigrateOutput,
    borealis_content::ContentStore,
) {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("in");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::write(input.join("a.xml"), xml).unwrap();
    let out = migrate_path(&input, &MigrateOptions::default()).unwrap();
    let dir = tmp.path().join("out");
    write_output(&dir, &out).unwrap();
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&dir)
        .unwrap()
        .build();
    (out, store)
}

#[test]
fn unsafe_homebrew_ids_are_normalised_with_alias_and_info() {
    let (out, store) = migrate_and_load(BROKEN_HOMEBREW);
    assert!(out
        .report
        .issues
        .iter()
        .all(|i| i.code != IssueCode::InvalidId));
    assert!(store
        .validate()
        .iter()
        .all(|i| i.code != IssueCode::InvalidId));
    let notes: Vec<_> = out
        .report
        .issues
        .iter()
        .filter(|i| i.code == IssueCode::Normalized && i.message.starts_with("id normalised"))
        .collect();
    assert_eq!(notes.len(), 4, "one Info per renamed element");
    assert!(notes.iter().all(|i| i.severity == Severity::Info));
    assert_eq!(store.len(), 5, "no element is dropped");
    // The original Aurora id still resolves, to the safe element.
    let e = store.element("ID_HB_FEAT_APOS'S_ID").expect("alias");
    assert_eq!(e.id, "ID_HB_FEAT_APOS_S_ID");
    assert_eq!(e.renamed_from.as_deref(), Some("ID_HB_FEAT_APOS'S_ID"));
    assert_eq!(
        store.get("ID_HB_FEAT_FINE").unwrap().origin,
        PackOrigin::Custom
    );
}

const REFERENCED: &str = r#"<elements>
  <element name="Odd" type="Feat" source="Hb" id="ID_HB_FEAT_ODD_(X)"><description><p>a</p></description></element>
  <element name="Odd2" type="Feat" source="Hb" id="ID_HB_FEAT_ODD__X_"><description><p>clash</p></description></element>
  <element name="User" type="Feat" source="Hb" id="ID_HB_FEAT_USER">
    <requirements>[ID_HB_FEAT_ODD_(X)]</requirements>
    <rules>
      <grant type="Feat" id="ID_HB_FEAT_ODD_(X)" />
      <select type="Feat" name="Pick" supports="ID_HB_FEAT_ODD_(X)">
        <item id="ID_HB_FEAT_ODD_(X)" />
      </select>
    </rules>
  </element>
</elements>"#;

#[test]
fn references_follow_the_rename_and_collisions_get_a_stable_suffix() {
    let (out, store) = migrate_and_load(REFERENCED);
    let again = migrate_and_load(REFERENCED).0;
    assert_eq!(out.files, again.files, "deterministic");
    let odd = store.element("ID_HB_FEAT_ODD_(X)").expect("alias resolves");
    assert_ne!(odd.id, "ID_HB_FEAT_ODD__X_", "collision-safe");
    assert!(odd.id.starts_with("ID_HB_FEAT_ODD__X__"));
    assert_eq!(store.element("ID_HB_FEAT_ODD__X_").unwrap().name, "Odd2");
    let user = store.element("ID_HB_FEAT_USER").unwrap();
    assert!(user.requirements.contains(&odd.id), "{}", user.requirements);
    assert!(!user.requirements.contains('('));
    let dangling = store
        .validate()
        .into_iter()
        .filter(|i| i.code == IssueCode::DanglingReference || i.code == IssueCode::InvalidId)
        .count();
    assert_eq!(dangling, 0, "every reference was rewritten");
}

#[test]
fn shadowing_stubs_are_diagnosed_not_silently_accepted() {
    // A Level without its required setters makes Aurora discard the whole custom file
    // (content-format 9.1): nothing of it loads, and every element is named in the report.
    let (out, store) = migrate_and_load(STUBS);
    let codes: Vec<_> = out.report.issues.iter().map(|i| i.code).collect();
    assert!(codes.contains(&IssueCode::MissingRequiredSetter));
    assert!(codes.contains(&IssueCode::FileSkipped));
    for id in ["ID_HB_LEVEL_1", "ID_HB_CLASS_X", "ID_HB_MULTICLASS_X"] {
        assert!(store.get(id).is_none(), "{id} not loaded");
        assert!(
            out.report
                .issues
                .iter()
                .any(|i| i.code == IssueCode::ElementDropped && i.element.as_deref() == Some(id)),
            "{id} named"
        );
    }
}
