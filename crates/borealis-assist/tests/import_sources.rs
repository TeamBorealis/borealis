//! Source ingestion and section locating: large and scanned PDFs, office documents, contents
//! pages that name elements before their sections, two-column pages. All text is invented; the
//! model is a scripted fake (no CLI is started).

mod import_fake;

use std::sync::Mutex;

use borealis_ai::task::{AiRunner, BoxFuture};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest, Attachment, AttachmentKind};
use borealis_assist::import::anything::{detect_source, ImportSource};
use borealis_assist::import::chunk_text;
use borealis_assist::import::testing::{synthetic_pdf, synthetic_zip, Piece};
use borealis_assist::import::{extract_pdf_text, office_text, OfficeFormat};
use borealis_assist::{
    finalize_draft, import_anything, Edition, ImportDraft, ImportStep, VisionRunner,
};
use import_fake::*;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

const MB: usize = 1024 * 1024;

/// The fake, plus the byte size of every attachment it was sent.
struct Sizing {
    fake: Fake,
    sizes: Mutex<Vec<usize>>,
}

impl AiRunner for Sizing {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.fake.run(req, sink)
    }
}

impl VisionRunner for Sizing {
    fn run_with<'a>(
        &'a self,
        req: &'a AiRequest,
        attachments: &'a [Attachment],
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.sizes
            .lock()
            .unwrap()
            .extend(attachments.iter().map(|a| a.bytes.len()));
        self.fake.run_with(req, attachments, sink)
    }
}

fn feats_detect(names: &[&str]) -> String {
    let items: Vec<Value> = names
        .iter()
        .map(|n| json!({"kind": "feat", "name": n}))
        .collect();
    json!({"kind": "feat", "edition": "2024", "confidence": 0.9, "items": items}).to_string()
}

fn transcript(text: &str, headings: &[&str]) -> String {
    json!({"text": text, "headings": headings}).to_string()
}

fn pdf_attachment(bytes: Vec<u8>) -> Attachment {
    Attachment {
        kind: AttachmentKind::Pdf,
        bytes,
        name: "almanac.pdf".into(),
    }
}

#[tokio::test]
async fn a_pdf_over_20_mb_is_read_from_its_text_layer_and_scanned_parts() {
    // Pages 1-3 have a text layer, pages 4-11 are scans (images only); 2 MB of image per page.
    let mut pages: Vec<Vec<Piece<'_>>> = vec![
        vec![
            (72.0, 720.0, 18.0, true, "Ashen Rites"),
            (
                72.0,
                690.0,
                10.0,
                false,
                "The first rite is spoken at dusk by the eldest.",
            ),
        ],
        vec![(
            72.0,
            700.0,
            10.0,
            false,
            "The rite continues on the next page with ash.",
        )],
        vec![
            (72.0, 720.0, 18.0, true, "Ember Oath"),
            (
                72.0,
                690.0,
                10.0,
                false,
                "An oath sworn over a dying fire, never broken.",
            ),
        ],
    ];
    pages.extend((0..8).map(|_| Vec::new()));
    let pdf = synthetic_pdf(&pages, 2 * MB);
    assert!(pdf.len() > 20 * MB, "{} bytes", pdf.len());

    let partial = transcript(
        "# Cinder Ward\n\nAsh rises as a wall.",
        &["Cinder Ward", "Smoke Veil"],
    );
    let full_a = transcript(
        "# Cinder Ward\n\nAsh rises as a wall.\n\n# Smoke Veil\n\nGrey smoke hides you.",
        &["Cinder Ward", "Smoke Veil"],
    );
    let full_b = transcript("# Soot Mark\n\nA mark of soot on the brow.", &["Soot Mark"]);
    let fake = Fake::default()
        .on("import_transcribe", "pages 4-7", &[partial, full_a])
        .on("import_transcribe", "pages 8-11", &[full_b])
        .on(
            "import_detect",
            "Classify",
            &[feats_detect(&["Cinder Ward", "Smoke Veil", "Soot Mark"])],
        );
    let runner = Sizing {
        fake,
        sizes: Mutex::new(Vec::new()),
    };
    let source = ImportSource::Attachments(vec![pdf_attachment(pdf)]);
    let (detection, text) = detect_source(&runner, &source, Edition::Y2024, &opts())
        .await
        .unwrap();

    // Text-layer pages need no model call; the 8 scanned pages go in 2 parts of 4, the first
    // asked again because a listed heading was missing from its text.
    assert_eq!(runner.fake.calls("import_transcribe"), 3);
    let sizes = runner.sizes.lock().unwrap().clone();
    assert_eq!(sizes.len(), 3);
    assert!(
        sizes.iter().all(|s| *s < 20 * MB && *s > 4 * MB),
        "{sizes:?}"
    );
    let order: Vec<usize> = [
        "# Ashen Rites",
        "next page with ash",
        "# Ember Oath",
        "# Smoke Veil",
        "# Soot Mark",
    ]
    .iter()
    .map(|s| {
        text.find(s)
            .unwrap_or_else(|| panic!("{s} missing from:\n{text}"))
    })
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{text}");
    assert!(
        detection
            .notes
            .iter()
            .all(|n| !n.starts_with("Reading the source")),
        "{:?}",
        detection.notes
    );
}

#[tokio::test]
async fn a_scanned_part_that_stays_incomplete_is_reported() {
    let pdf = synthetic_pdf(&[Vec::new(), Vec::new()], 1000);
    let partial = transcript(
        "# Cinder Ward\n\nAsh rises.",
        &["Cinder Ward", "Smoke Veil"],
    );
    let fake = Fake::default()
        .on("import_transcribe", "pages 1-2", &[partial])
        .on(
            "import_detect",
            "Classify",
            &[feats_detect(&["Cinder Ward"])],
        );
    let source = ImportSource::Attachments(vec![pdf_attachment(pdf)]);
    let (detection, _) = detect_source(&fake, &source, Edition::Y2024, &opts())
        .await
        .unwrap();
    assert_eq!(fake.calls("import_transcribe"), 2);
    let note = detection
        .notes
        .iter()
        .find(|n| n.starts_with("Reading the source"))
        .expect("a reading note");
    assert!(
        note.contains("Smoke Veil") && note.contains("pages 1-2"),
        "{note}"
    );
}

/// Invented book: a cover line and a contents page name every feat before its section; one feat
/// has no section at all; a section spans paragraphs, a sub-heading and a page break.
const BOOK: &str = "# The Lantern Almanac\n\nQuick Draw – Steady Hand – Iron Gut\n\n## Contents\n\n\
Quick Draw 4\nSteady Hand 5\nIron Gut 6\nIndex 9\n\n## Feats\n\n### Quick Draw\n\n\
You draw a weapon as part of any attack.\n\nWhile holding a lantern you also gain +1 initiative.\n\n\
#### Variant\n\nA table may allow two draws.\n\n### Steady Hand\n\nYour aim does not waver.\n\n\
## Rules of the Road\n\nTravel is slow.\n";

async fn draft_for(fake: &Fake, source: &str) -> ImportDraft {
    let req = request(ImportSource::Text(source.into()), "lan", Edition::Y2024);
    match import_anything(fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    {
        ImportStep::Draft(d) | ImportStep::NeedsChoice(d) => *d,
    }
}

#[tokio::test]
async fn contents_pages_are_skipped_and_whole_sections_are_sent() {
    let feats = batch_reply(&[
        el_reply("Quick Draw", "Feat"),
        el_reply("Steady Hand", "Feat"),
        el_reply("Iron Gut", "Feat"),
    ]);
    let fake = Fake::default()
        .on(
            "import_detect",
            "Classify",
            &[feats_detect(&["Quick Draw", "Steady Hand", "Iron Gut"])],
        )
        .on("import_generate", "of type Feat (", &[feats]);
    let d = draft_for(&fake, BOOK).await;
    let line = |n: &str| {
        d.plan
            .elements
            .iter()
            .find(|p| p.name == n)
            .unwrap()
            .excerpt
            .clone()
    };
    let quick = line("Quick Draw").expect("Quick Draw located");
    let lines: Vec<&str> = BOOK.lines().collect();
    assert_eq!(lines[quick.start_line - 1], "### Quick Draw");
    let section = lines[quick.start_line - 1..quick.end_line].join("\n");
    assert!(
        section.contains("+1 initiative") && section.contains("two draws"),
        "{section}"
    );
    assert!(!section.contains("Steady Hand"), "{section}");
    // Iron Gut is only named on the cover, in the contents and nowhere else: no section, and
    // the report says so (even though the source is short).
    assert!(line("Iron Gut").is_none(), "{:?}", line("Iron Gut"));
    let report = finalize_draft(&base(), &d).unwrap().pack.report.unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.message.contains("Iron Gut")),
        "{:?}",
        report.warnings
    );
    assert!(!report
        .warnings
        .iter()
        .any(|w| w.message.contains("Quick Draw")));
}

#[tokio::test]
async fn two_column_pages_keep_a_section_together() {
    // Left column: a heading and the start of a section; right column: its end, then the next.
    let mut page: Vec<Piece<'_>> = vec![(72.0, 740.0, 16.0, true, "Moonlit Step")];
    let left = [
        "You step between shadows",
        "as a bonus action and",
        "reappear up to 30 feet",
        "away in dim light.",
    ];
    let right = ["The step leaves frost.", "", "", ""];
    for i in 0..4 {
        let y = 712.0 - 14.0 * i as f32;
        page.push((72.0, y, 10.0, false, left[i]));
        if !right[i].is_empty() {
            page.push((330.0, y, 10.0, false, right[i]));
        }
    }
    page.push((330.0, 670.0, 16.0, true, "Starlit Ward"));
    page.push((330.0, 650.0, 10.0, false, "A ward of starlight wraps"));
    page.push((330.0, 636.0, 10.0, false, "you until dawn."));
    let text = extract_pdf_text(&synthetic_pdf(&[page], 0)).unwrap().text;
    let feats = batch_reply(&[
        el_reply("Moonlit Step", "Feat"),
        el_reply("Starlit Ward", "Feat"),
    ]);
    let fake = Fake::default()
        .on(
            "import_detect",
            "Classify",
            &[feats_detect(&["Moonlit Step", "Starlit Ward"])],
        )
        .on("import_generate", "of type Feat (", &[feats]);
    let d = draft_for(&fake, &text).await;
    let lines: Vec<&str> = text.lines().collect();
    let section = |n: &str| {
        let ex = d
            .plan
            .elements
            .iter()
            .find(|p| p.name == n)
            .unwrap()
            .excerpt
            .clone()
            .unwrap();
        lines[ex.start_line - 1..ex.end_line].join("\n")
    };
    let step = section("Moonlit Step");
    assert!(
        step.contains("away in dim light.") && step.contains("leaves frost"),
        "{text}"
    );
    assert!(!step.contains("starlight"), "{step}");
    assert!(section("Starlit Ward").contains("until dawn"), "{text}");
}

fn docx(body: &str) -> Vec<u8> {
    let doc = format!(r#"<w:document xmlns:w="w"><w:body>{body}</w:body></w:document>"#);
    let styles = r#"<w:styles xmlns:w="w"><w:style w:styleId="Heading1"><w:name w:val="heading 1"/></w:style><w:style w:styleId="Heading2"><w:name w:val="heading 2"/></w:style></w:styles>"#;
    synthetic_zip(&[
        ("word/document.xml", doc.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
    ])
}

fn p(style: Option<&str>, text: &str) -> String {
    let ppr = style
        .map(|s| format!(r#"<w:pPr><w:pStyle w:val="{s}"/></w:pPr>"#))
        .unwrap_or_default();
    format!("<w:p>{ppr}<w:r><w:t>{text}</w:t></w:r></w:p>")
}

#[test]
fn office_documents_keep_every_heading_and_table_row() {
    let mut body = p(Some("Heading1"), "Hearth Spirits");
    let names = ["Kindle Friend", "Ash Guardian", "Coal Whisper"];
    for n in names {
        body.push_str(&p(Some("Heading2"), n));
        body.push_str(&p(None, &format!("{n} is a small spirit of the hearth.")));
    }
    body.push_str("<w:tbl><w:tr><w:tc><w:p><w:r><w:t>d6</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Mood</w:t></w:r></w:p></w:tc></w:tr></w:tbl>");
    let text = office_text(OfficeFormat::Docx, &docx(&body)).unwrap();
    for n in names {
        assert!(text.contains(&format!("## {n}")), "{text}");
    }
    assert!(text.contains("d6 | Mood"));
    // Coverage of the chunked old-pipeline path: every heading starts a chunk.
    let heads: Vec<String> = chunk_text(&text)
        .into_iter()
        .filter_map(|c| c.heading)
        .collect();
    for n in names {
        assert!(heads.iter().any(|h| h == n), "{heads:?}");
    }

    let odt = synthetic_zip(&[(
        "content.xml",
        b"<office:document-content><office:body><office:text><text:h text:outline-level=\"1\">Hearth Spirits</text:h><text:h text:outline-level=\"2\">Kindle Friend</text:h><text:p>A spark that purrs.</text:p></office:text></office:body></office:document-content>",
    )]);
    let text = office_text(OfficeFormat::Odt, &odt).unwrap();
    assert_eq!(
        text,
        "# Hearth Spirits\n\n## Kindle Friend\n\nA spark that purrs."
    );
    assert!(office_text(OfficeFormat::Docx, &odt).is_err());
}
