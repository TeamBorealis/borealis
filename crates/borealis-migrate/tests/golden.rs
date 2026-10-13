//! Golden tests: each XML sample from the spec converts to the JSON that sits beside it.

mod common;

use borealis_migrate::{convert_xml, Document};
use serde_json::Value;

#[test]
fn spec_samples_convert_to_the_documented_json() {
    let Some(dir) = common::require(
        "spec samples",
        common::samples_dir(),
        "BOREALIS_SPEC_SAMPLES",
    ) else {
        return;
    };
    let xmls = common::files_with_ext(&dir, "xml");
    assert_eq!(xmls.len(), 9, "expected the nine spec samples");
    for xml_path in xmls {
        let name = xml_path.file_name().unwrap().to_string_lossy().to_string();
        let xml = std::fs::read_to_string(&xml_path).unwrap();
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(xml_path.with_extension("json")).unwrap(),
        )
        .unwrap();

        let conv = convert_xml(&xml, &name);
        let errors: Vec<_> = conv
            .issues
            .iter()
            .filter(|i| i.severity == borealis_content::Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{name}: {errors:#?}");
        let Some(Document::Content(file)) = conv.document else {
            panic!("{name}: no content")
        };
        let actual = serde_json::to_value(&file).unwrap();
        assert_eq!(actual, expected, "{name} differs from the spec sample");
    }
}

#[test]
fn spec_sample_json_parses_into_the_model_and_validates() {
    let Some(dir) = common::require(
        "spec samples",
        common::samples_dir(),
        "BOREALIS_SPEC_SAMPLES",
    ) else {
        return;
    };
    for path in common::files_with_ext(&dir, "json") {
        let text = std::fs::read_to_string(&path).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let violations = borealis_content::schema::validate_content(&value).unwrap();
        assert!(violations.is_empty(), "{}: {violations:#?}", path.display());
        let file = borealis_content::ContentFile::from_json_str(&text).unwrap();
        assert_eq!(
            serde_json::to_value(&file).unwrap(),
            value,
            "{}",
            path.display()
        );
    }
}
