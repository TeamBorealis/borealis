//! Pages read visually (no text layer): what the transcription says about itself never enters
//! the source as printed text, reworded passages are asked for once more, and what stays
//! reworded is recorded passage by passage for the review. All text is invented; the model is
//! a scripted fake.

mod import_fake;

use borealis_ai::{Attachment, AttachmentKind};
use borealis_assist::import::anything::{detect_source, ImportSource};
use borealis_assist::import::testing::synthetic_pdf;
use borealis_assist::Edition;
use import_fake::*;
use serde_json::json;

fn transcript(text: &str, headings: &[&str]) -> String {
    json!({"text": text, "headings": headings}).to_string()
}

fn detected() -> String {
    json!({"kind": "feat", "edition": "2024", "confidence": 0.9,
        "items": [{"kind": "feat", "name": "Cinder Ward"}]})
    .to_string()
}

#[tokio::test]
async fn a_reworded_scan_is_asked_once_more_and_what_stays_reworded_is_recorded() {
    // Arrange: two scanned pages; the first copy notes the cover and rewords two passages, the
    // second copies one of them word for word.
    let pdf = synthetic_pdf(&[Vec::new(), Vec::new()], 1000);
    let first = transcript(
        "[Note: Page 1 is the cover. It shows a lantern over a moor.]\n\n# Cinder Ward\n\n[Paraphrased] Ash rises around you when you are hit.\n\nThe ward lasts until you finish a long rest. (Paraphrased)",
        &["Cinder Ward"],
    );
    let second = transcript(
        "# Cinder Ward\n\n[Paraphrased] Ash rises around you when you are hit.\n\nThe ward lasts until you finish a Long Rest.",
        &["Cinder Ward"],
    );
    let fake = Fake::default()
        .on("import_transcribe", "pages 1-2", &[first, second])
        .on("import_detect", "Classify", &[detected()]);
    let source = ImportSource::Attachments(vec![Attachment {
        kind: AttachmentKind::Pdf,
        bytes: pdf,
        name: "ward.pdf".into(),
    }]);
    // Act
    let (detection, text) = detect_source(&fake, &source, Edition::Y2024, &opts())
        .await
        .unwrap();
    // Assert: the retry says what was reworded; the copy with fewer reworded passages is kept,
    // without markers or the cover note.
    assert_eq!(fake.calls("import_transcribe"), 2);
    let retry = &fake.prompts("import_transcribe")[1];
    assert!(
        retry.contains("2 passage(s) were reworded instead of copied"),
        "{retry}"
    );
    assert!(text.contains("until you finish a Long Rest."), "{text}");
    assert!(
        !text.contains("cover") && !text.to_lowercase().contains("paraphrased"),
        "{text}"
    );
    let recorded: Vec<&String> = detection
        .notes
        .iter()
        .filter(|n| n.starts_with("Reading the source: paraphrased passage: "))
        .collect();
    assert_eq!(recorded.len(), 1, "{:?}", detection.notes);
    assert!(
        recorded[0]
            .ends_with("\"Ash rises around you when you are hit.\" (pages 1-2 of `ward.pdf`)"),
        "{}",
        recorded[0]
    );
}
