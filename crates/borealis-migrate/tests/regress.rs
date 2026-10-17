//! Synthetic regression fixtures for constructs seen in real-world community content.
//! Everything here is invented; no corpus text is used.

use borealis_content::{IssueCode, Severity};
use borealis_migrate::{convert_xml, Document, FileConversion};

fn convert(body: &str) -> FileConversion {
    convert_xml(&format!("<elements>{body}</elements>"), "t.xml")
}

fn only_element(conv: &FileConversion) -> &borealis_content::Element {
    let Some(Document::Content(file)) = &conv.document else {
        panic!("no content document: {:?}", conv.issues)
    };
    assert_eq!(file.elements.len(), 1);
    &file.elements[0]
}

fn count(conv: &FileConversion, severity: Severity, code: IssueCode) -> usize {
    conv.issues
        .iter()
        .filter(|i| i.severity == severity && i.code == code)
        .count()
}

#[test]
fn misspelled_tags_and_attributes_are_repaired_with_a_warning() {
    let conv = convert(
        r#"<element name="A" type="T" source="S" id="ID_A">
             <suports>X, Y</suports>
             <descripton><p>text</p></descripton>
             <shet useage="1/Long Rest"><description level="2">sheet text</description></shet>
             <spellcastin name="W"/>
           </element>"#,
    );
    let e = only_element(&conv);
    assert_eq!(e.supports, ["X", "Y"]);
    assert!(e.description.contains("text"));
    let sheet = e.sheet.as_ref().expect("sheet recovered");
    assert_eq!(sheet.usage.as_deref(), Some("1/Long Rest"));
    assert!(e.spellcasting.is_some());
    assert_eq!(count(&conv, Severity::Warning, IssueCode::Normalized), 5);
    assert!(e.extra.children.is_empty());
}

#[test]
fn attribute_repair_never_overrides_the_correct_attribute() {
    let conv = convert(
        r#"<element name="A" type="T" source="S" id="ID_A">
             <sheet usage="right" useage="wrong"/></element>"#,
    );
    let sheet = only_element(&conv).sheet.clone().unwrap();
    assert_eq!(sheet.usage.as_deref(), Some("right"));
    assert_eq!(sheet.extra.attributes["useage"], "wrong");
}

#[test]
fn grant_without_target_drops_only_that_rule() {
    let conv = convert(
        r#"<element name="A" type="T" source="S" id="ID_A">
             <rules><grant type="T"/><grant type="T" id="ID_OK"/></rules></element>"#,
    );
    let e = only_element(&conv);
    assert_eq!(e.rules.len(), 1);
    assert_eq!(conv.stats.dropped_elements, 0);
    assert_eq!(conv.stats.dropped_rules, 1);
    assert_eq!(
        count(&conv, Severity::Warning, IssueCode::MissingAttribute),
        1
    );
    assert_eq!(
        count(&conv, Severity::Error, IssueCode::MissingAttribute),
        0
    );
}

#[test]
fn aurora_ignored_attributes_are_info_and_unknown_ones_warn() {
    let conv = convert(
        r#"<element name="A" type="T" source="S" id="ID_A">
             <sheet name="label"/>
             <rules><grant id="ID_G" known="true" supports="X" mystery="1"/></rules></element>"#,
    );
    let e = only_element(&conv);
    // preserved losslessly either way
    assert_eq!(e.sheet.as_ref().unwrap().extra.attributes["name"], "label");
    let infos = count(&conv, Severity::Info, IssueCode::UnsupportedConstruct);
    let warns = count(&conv, Severity::Warning, IssueCode::UnsupportedConstruct);
    assert_eq!((infos, warns), (3, 1), "{:?}", conv.issues);
}

#[test]
fn vendor_namespaced_and_stray_markup_children_are_info() {
    let conv = convert(
        r#"<p>stray</p><al:corrections x="1"/><oddity/>
           <element name="A" type="T" source="S" id="ID_A"/>"#,
    );
    assert_eq!(
        count(&conv, Severity::Info, IssueCode::UnsupportedConstruct),
        2
    );
    assert_eq!(
        count(&conv, Severity::Warning, IssueCode::UnsupportedConstruct),
        1
    );
}

#[test]
fn repeated_support_tags_are_info_and_repeated_sheets_keep_the_first() {
    let conv = convert(
        r#"<element name="A" type="T" source="S" id="ID_A">
             <supports>X, X, Y</supports>
             <sheet alt="first"/><sheet alt="second"/></element>"#,
    );
    let e = only_element(&conv);
    assert_eq!(e.supports, ["X", "Y"]);
    assert_eq!(e.sheet.as_ref().unwrap().alt.as_deref(), Some("first"));
    assert_eq!(e.extra.children.len(), 1, "the second sheet is not lost");
    assert_eq!(count(&conv, Severity::Info, IssueCode::DuplicateChild), 1);
    assert_eq!(
        count(&conv, Severity::Warning, IssueCode::DuplicateChild),
        1
    );
}

#[test]
fn empty_and_foreign_files_are_skipped_cleanly() {
    for (name, text, severity) in [
        ("e.index", "", Severity::Warning),
        ("e.xml", "  \n\t", Severity::Info),
        (
            "m.xml",
            "<manifest><uses-permission/></manifest>",
            Severity::Info,
        ),
    ] {
        let conv = convert_xml(text, name);
        assert!(conv.document.is_none(), "{name}");
        assert_eq!(conv.issues.len(), 1, "{name}");
        assert_eq!(conv.issues[0].code, IssueCode::FileSkipped, "{name}");
        assert_eq!(conv.issues[0].severity, severity, "{name}");
    }
}
