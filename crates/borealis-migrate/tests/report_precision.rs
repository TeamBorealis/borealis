//! IM4 stage 1: every report issue is precise (file, element id, element type, code), and no
//! source element disappears without a trace (converted, renamed with an alias, or reported).

use borealis_content::{IssueCode, Severity};
use borealis_migrate::{convert_xml, migrate_path, MigrateOptions, MigrateOutput};

fn migrate(files: &[(&str, &str)]) -> MigrateOutput {
    let tmp = tempfile::tempdir().unwrap();
    for (name, xml) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, xml).unwrap();
    }
    migrate_path(tmp.path(), &MigrateOptions::default()).unwrap()
}

const QUIRKS: &str = r#"<elements>
  <element name="Odd" type="Feat" source="Hb" id="ID_HB_FEAT_ODD">
    <description><p>a</p></description>
    <setter name="x">1</setter>
    <sheet><description>one</description></sheet>
    <sheet><description>two</description></sheet>
    <set name="y">2</set>
    <rules><grant type="Feat" id="ID_HB_FEAT_FINE" requirments="[ID_HB_FEAT_FINE]" /></rules>
  </element>
  <element name="Fine" type="Feat" source="Hb" id="ID_HB_FEAT_FINE"><description><p>b</p></description></element>
</elements>"#;

#[test]
fn warnings_inside_an_element_name_that_element() {
    let out = migrate(&[("sub/quirks.xml", QUIRKS)]);
    let warnings: Vec<_> = out
        .report
        .issues
        .iter()
        .filter(|i| i.severity != Severity::Info)
        .collect();
    assert!(warnings.len() >= 4, "{warnings:#?}");
    for w in &warnings {
        assert_eq!(w.element.as_deref(), Some("ID_HB_FEAT_ODD"), "{w:?}");
        assert_eq!(w.file.as_deref(), Some("sub/quirks.xml"), "{w:?}");
        assert_eq!(out.report.element_type("ID_HB_FEAT_ODD"), Some("Feat"));
    }
}

#[test]
fn file_level_issues_after_an_element_are_not_attributed_to_it() {
    let xml = r#"<elements>
  <element name="A" type="Feat" source="Hb" id="ID_HB_FEAT_A"><description><p>a</p></description></element>
  <oddity />
</elements>"#;
    let conv = convert_xml(xml, "a.xml");
    let odd = conv
        .issues
        .iter()
        .find(|i| i.message.contains("oddity"))
        .expect("unknown file-level child is reported");
    assert_eq!(odd.element, None);
}

#[test]
fn element_types_cover_converted_dropped_and_renamed_elements() {
    let xml = r#"<elements>
  <element name="Kept" type="Spell" source="Hb" id="ID_HB_SPELL_KEPT"><description><p>a</p></description><setters><set name="level">1</set><set name="school">Evocation</set><set name="time">1 action</set><set name="duration">Instantaneous</set><set name="range">Self</set></setters></element>
  <element name="" type="Race" source="Hb" id="ID_HB_RACE_NONAME"><description><p>b</p></description></element>
  <element name="Sniper" type="Feat" source="Hb" id="ID_HB_FEAT_SNIPER'S_NEST"><description><p>c</p></description></element>
</elements>"#;
    let out = migrate(&[("a.xml", xml)]);
    let r = &out.report;
    assert_eq!(r.element_type("ID_HB_SPELL_KEPT"), Some("Spell"));
    assert_eq!(r.element_type("ID_HB_RACE_NONAME"), Some("Race"));
    let dropped = r
        .issues
        .iter()
        .find(|i| i.code == IssueCode::MissingAttribute)
        .expect("dropped element reported");
    assert_eq!(dropped.element.as_deref(), Some("ID_HB_RACE_NONAME"));
    assert_eq!(dropped.severity, Severity::Error);
    assert_eq!(r.element_type("ID_HB_FEAT_SNIPER'S_NEST"), Some("Feat"));
    assert_eq!(r.element_type("ID_HB_FEAT_SNIPER_S_NEST"), Some("Feat"));
}

#[test]
fn a_renamed_id_is_reported_under_its_source_id_with_the_new_id() {
    let xml = r#"<elements>
  <element name="Sniper" type="Feat" source="Hb" id="ID_HB_FEAT_SNIPER'S_NEST"><description><p>c</p></description></element>
</elements>"#;
    let out = migrate(&[("a.xml", xml)]);
    let note = out
        .report
        .issues
        .iter()
        .find(|i| i.code == IssueCode::Normalized && i.message.starts_with("id normalised"))
        .expect("rename note");
    assert_eq!(note.element.as_deref(), Some("ID_HB_FEAT_SNIPER'S_NEST"));
    assert!(
        note.message.contains("ID_HB_FEAT_SNIPER_S_NEST"),
        "{}",
        note.message
    );
    assert_eq!(note.file.as_deref(), Some("a.xml"));
    assert_eq!(
        out.report
            .renamed
            .get("ID_HB_FEAT_SNIPER'S_NEST")
            .map(String::as_str),
        Some("ID_HB_FEAT_SNIPER_S_NEST")
    );
}

#[test]
fn a_file_that_does_not_parse_reports_every_element_it_held() {
    let xml = r#"<elements>
  <element name="One" type="Feat" source="Hb" id="ID_HB_FEAT_ONE"><description><p>a</p></description></element>
  <element name='Two' type='Spell' source="Hb" id='ID_HB_SPELL_TWO'><description><p>b</description></element>
</elements>"#;
    let out = migrate(&[("broken.xml", xml)]);
    let r = &out.report;
    assert!(r
        .issues
        .iter()
        .any(|i| i.code == IssueCode::XmlParse && i.element.is_none()));
    let dropped: Vec<_> = r
        .issues
        .iter()
        .filter(|i| i.code == IssueCode::ElementDropped)
        .collect();
    let ids: Vec<_> = dropped
        .iter()
        .filter_map(|i| i.element.as_deref())
        .collect();
    assert_eq!(ids, ["ID_HB_FEAT_ONE", "ID_HB_SPELL_TWO"]);
    assert!(dropped.iter().all(|i| i.severity == Severity::Error
        && i.file.as_deref() == Some("broken.xml")
        && i.line.is_some()));
    assert_eq!(r.element_type("ID_HB_SPELL_TWO"), Some("Spell"));
    assert_eq!(r.totals.dropped_elements, 2);
}

#[test]
fn an_index_saved_as_xml_is_validated_as_an_index() {
    let index = r#"<index><info><name>Hb</name></info><files><file name="a.xml" url="https://example.invalid/a.xml" /></files></index>"#;
    let out = migrate(&[("listing.xml", index)]);
    let schema: Vec<_> = out
        .report
        .issues
        .iter()
        .filter(|i| i.code == IssueCode::SchemaViolation)
        .collect();
    assert!(schema.is_empty(), "{schema:#?}");
    assert_eq!(out.report.summary.index_files, 1);
}

#[test]
fn an_empty_file_is_a_note_not_a_warning() {
    let out = migrate(&[("empty.xml", "  \n")]);
    let note = out
        .report
        .issues
        .iter()
        .find(|i| i.code == IssueCode::FileSkipped)
        .expect("empty file noted");
    assert_eq!(note.severity, Severity::Info);
    assert_eq!(note.file.as_deref(), Some("empty.xml"));
}
