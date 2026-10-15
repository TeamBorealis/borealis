//! Pictures on pages that have a text layer (a stat block pasted as an image): their text is
//! copied into the source where the picture sits, so later stages read the printed numbers
//! instead of guessing them. All text is invented; the model is a scripted fake.

mod import_fake;

use borealis_ai::{Attachment, AttachmentKind};
use borealis_assist::import::anything::{detect_source, ImportSource};
use borealis_assist::Edition;
use import_fake::*;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde_json::json;

/// Text at (x, y, size, text).
type Line<'a> = (f32, f32, f32, &'a str);

/// Letter pages, each with its text and an optional picture `[x, y, width, height]`.
fn pdf(pages: &[(Vec<Line<'_>>, Option<[f32; 4]>)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(
        dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"},
    );
    let img = doc.add_object(
        Stream::new(
            dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8},
            vec![90u8; 12],
        )
        .with_compression(false),
    );
    let mut kids = Vec::new();
    for (lines, picture) in pages {
        let mut ops = Vec::new();
        if let Some([x, y, w, h]) = picture {
            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new(
                "cm",
                vec![
                    (*w).into(),
                    0.into(),
                    0.into(),
                    (*h).into(),
                    (*x).into(),
                    (*y).into(),
                ],
            ));
            ops.push(Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]));
            ops.push(Operation::new("Q", vec![]));
        }
        for (x, y, size, text) in lines {
            ops.push(Operation::new("BT", vec![]));
            ops.push(Operation::new(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), (*size).into()],
            ));
            ops.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
            ops.push(Operation::new("Tj", vec![Object::string_literal(*text)]));
            ops.push(Operation::new("ET", vec![]));
        }
        let content = doc.add_object(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode().unwrap(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {"Font" => dictionary! {"F1" => font},
                "XObject" => dictionary! {"Im1" => img}},
        });
        kids.push(page.into());
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => count}),
    );
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// Page 1: a creature's lore with its stat block as a picture below it, then a second
/// creature's heading. Page 2: plain text.
fn bestiary() -> Vec<u8> {
    pdf(&[
        (
            vec![
                (72.0, 730.0, 18.0, "Gloomwing"),
                (
                    72.0,
                    700.0,
                    10.0,
                    "Gloomwings roost under old bridges and hum at dusk.",
                ),
                (72.0, 200.0, 18.0, "Marsh Lantern"),
                (
                    72.0,
                    170.0,
                    10.0,
                    "Marsh lanterns drift over the reeds at night.",
                ),
            ],
            Some([72.0, 260.0, 240.0, 400.0]),
        ),
        (
            vec![
                (72.0, 730.0, 18.0, "Reed Wardens"),
                (
                    72.0,
                    700.0,
                    10.0,
                    "The wardens keep the causeways clear of weeds.",
                ),
            ],
            None,
        ),
    ])
}

fn attachment(bytes: Vec<u8>) -> ImportSource {
    ImportSource::Attachments(vec![Attachment {
        kind: AttachmentKind::Pdf,
        bytes,
        name: "bestiary.pdf".into(),
    }])
}

fn detected() -> String {
    json!({"kind": "monster", "edition": "2014", "confidence": 0.9,
        "items": [{"kind": "monster", "name": "Gloomwing"}]})
    .to_string()
}

const STAT_BLOCK: &str = "### Gloomwing\nTiny beast, unaligned\nArmor Class 13\nHit Points 7 (3d4)\nSpeed 10 ft., fly 40 ft.\nSTR | DEX | CON | INT | WIS | CHA\n3 (-4) | 16 (+3) | 10 (+0) | 2 (-4) | 12 (+1) | 4 (-3)\nChallenge 1/8 (25 XP)";

#[tokio::test]
async fn a_stat_block_picture_on_a_text_page_is_copied_under_its_creature() {
    // Arrange: the model copies the picture's stat block and says which line it follows.
    let blocks = json!({"blocks": [{"page": 1,
        "after": "Gloomwings roost under old bridges and hum at dusk.", "text": STAT_BLOCK}]})
    .to_string();
    let fake = Fake::default()
        .on(
            "import_transcribe_pictures",
            "the pictures on page 1 of",
            &[blocks],
        )
        .on("import_detect", "Classify", &[detected()]);
    // Act
    let (detection, text) = detect_source(&fake, &attachment(bestiary()), Edition::Y2014, &opts())
        .await
        .unwrap();
    // Assert: one pictures call for the one pictured page (text layer in the prompt), no
    // page re-read visually, and the stat block sits in the creature's own section.
    assert_eq!(fake.calls("import_transcribe_pictures"), 1);
    assert_eq!(fake.calls("import_transcribe"), 0);
    let prompt = &fake.prompts("import_transcribe_pictures")[0];
    assert!(prompt.contains("--- page 1 ---"), "{prompt}");
    assert!(!prompt.contains("Reed Wardens"), "{prompt}");
    let at = |s: &str| {
        text.find(s)
            .unwrap_or_else(|| panic!("{s} missing:\n{text}\n{:?}", detection.notes))
    };
    assert!(at("hum at dusk.") < at("Armor Class 13"));
    assert!(at("Armor Class 13") < at("Marsh Lantern"));
    assert!(at("Challenge 1/8 (25 XP)") < at("Reed Wardens"));
    assert_eq!(text.matches("# Gloomwing").count(), 1, "{text}");
    assert!(text.contains("**Gloomwing**\nTiny beast"), "{text}");
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
async fn pictures_without_text_leave_the_text_layer_as_it_is() {
    let fake = Fake::default()
        .on(
            "import_transcribe_pictures",
            "pictures",
            &[json!({"blocks": []}).to_string()],
        )
        .on("import_detect", "Classify", &[detected()]);
    let (_, text) = detect_source(&fake, &attachment(bestiary()), Edition::Y2014, &opts())
        .await
        .unwrap();
    // The same text as a PDF whose page shows no picture.
    let plain = pdf(&[
        (
            vec![
                (72.0, 730.0, 18.0, "Gloomwing"),
                (
                    72.0,
                    700.0,
                    10.0,
                    "Gloomwings roost under old bridges and hum at dusk.",
                ),
                (72.0, 200.0, 18.0, "Marsh Lantern"),
                (
                    72.0,
                    170.0,
                    10.0,
                    "Marsh lanterns drift over the reeds at night.",
                ),
            ],
            None,
        ),
        (
            vec![
                (72.0, 730.0, 18.0, "Reed Wardens"),
                (
                    72.0,
                    700.0,
                    10.0,
                    "The wardens keep the causeways clear of weeds.",
                ),
            ],
            None,
        ),
    ]);
    let fake_plain = Fake::default().on("import_detect", "Classify", &[detected()]);
    let (_, expected) = detect_source(&fake_plain, &attachment(plain), Edition::Y2014, &opts())
        .await
        .unwrap();
    assert_eq!(fake_plain.calls("import_transcribe_pictures"), 0);
    assert_eq!(text, expected);
}

#[tokio::test]
async fn a_failed_pictures_call_is_reported_and_the_import_goes_on() {
    // No scripted reply: the call fails.
    let fake = Fake::default().on("import_detect", "Classify", &[detected()]);
    let (detection, text) = detect_source(&fake, &attachment(bestiary()), Edition::Y2014, &opts())
        .await
        .unwrap();
    assert!(text.contains("hum at dusk."));
    let note = detection
        .notes
        .iter()
        .find(|n| n.starts_with("Reading the source"))
        .expect("a reading note");
    assert!(
        note.contains("the pictures on page 1") && note.contains("stat block"),
        "{note}"
    );
}
