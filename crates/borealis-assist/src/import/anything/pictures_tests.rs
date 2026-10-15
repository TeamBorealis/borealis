use std::collections::BTreeSet;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

use super::*;

/// Text at (x, y, size).
type Line<'a> = (f32, f32, f32, &'a str);

/// A PDF of letter pages, each with its text and an optional picture `[x, y, width, height]`.
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
            vec![200u8; 12],
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

fn lore<'a>() -> Vec<Line<'a>> {
    vec![
        (72.0, 720.0, 18.0, "Gloomwing"),
        (
            72.0,
            690.0,
            10.0,
            "Gloomwings roost under old bridges and hum at dusk.",
        ),
        (
            72.0,
            676.0,
            10.0,
            "Villagers leave them sweet water in clay bowls.",
        ),
    ]
}

fn all_pages(n: u32) -> BTreeSet<u32> {
    (1..=n).collect()
}

#[test]
fn a_picture_with_no_text_over_it_is_found_on_a_text_page() {
    // Arrange: page 1 has a stat-block-sized picture below its lore; page 2 only text.
    let bytes = pdf(&[(lore(), Some([72.0, 300.0, 230.0, 300.0])), (lore(), None)]);
    // Act
    let found = pictured_pages(&bytes, &all_pages(2));
    // Assert
    assert_eq!(found, BTreeSet::from([1]));
    // Pages not asked for are not looked at.
    assert!(pictured_pages(&bytes, &BTreeSet::from([2])).is_empty());
}

#[test]
fn backgrounds_text_boxes_and_small_images_are_not_pictures() {
    let bytes = pdf(&[
        // A parchment box drawn behind the lore: text lies over it.
        (lore(), Some([50.0, 600.0, 500.0, 160.0])),
        // A whole-page background.
        (lore(), Some([0.0, 0.0, 612.0, 792.0])),
        // A small ornament.
        (lore(), Some([300.0, 300.0, 60.0, 60.0])),
        // A thin divider.
        (lore(), Some([72.0, 500.0, 400.0, 8.0])),
    ]);
    assert!(pictured_pages(&bytes, &all_pages(4)).is_empty());
}

#[test]
fn an_unreadable_pdf_has_no_pictures() {
    assert!(pictured_pages(b"not a pdf", &all_pages(1)).is_empty());
}

fn inset(page: u32, after: &str, text: &str) -> Inset {
    Inset {
        page,
        after: after.into(),
        text: text.into(),
    }
}

const RUN: &str = "# Gloomwing\n\nGloomwings roost under old bridges.\nThey hum at dusk.\n\n# Marsh Lights\n\nLights drift over the reeds.\n\nA second paragraph of page two.";

fn run_pages() -> Vec<(u32, String)> {
    vec![
        (
            4,
            "# Gloomwing\n\nGloomwings roost under old bridges.\nThey hum at dusk.".into(),
        ),
        (
            5,
            "# Marsh Lights\n\nLights drift over the reeds.\n\nA second paragraph of page two."
                .into(),
        ),
    ]
}

#[test]
fn a_picture_is_spliced_after_its_anchor_line_and_keeps_its_section() {
    // Arrange: the stat block picture repeats the lore heading above it.
    let block = "### Gloomwing\nSmall beast, unaligned\nArmor Class 13\nHit Points 9 (2d6 + 2)";
    // Act
    let out = splice(RUN, &run_pages(), &[inset(4, "They hum at dusk.", block)]);
    // Assert: placed after the anchor, before the next page; the repeated heading is bold, so
    // the stat block stays in the creature's section.
    let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("{s} missing:\n{out}"));
    assert!(at("They hum at dusk.") < at("Armor Class 13"));
    assert!(at("Armor Class 13") < at("# Marsh Lights"));
    assert!(out.contains("\n\n**Gloomwing**\nSmall beast"), "{out}");
    assert_eq!(out.matches("# Gloomwing").count(), 1, "{out}");
    assert!(!out.contains("\n\n\n"), "{out:?}");
}

#[test]
fn a_picture_without_a_found_anchor_goes_to_the_end_of_its_page() {
    let block = "### Reed Stalker\nArmor Class 14";
    let out = splice(
        RUN,
        &run_pages(),
        &[inset(4, "a line the page lacks", block)],
    );
    // A different name keeps its own heading.
    let stalker = out.find("### Reed Stalker").unwrap();
    assert!(stalker > out.find("They hum at dusk.").unwrap());
    assert!(stalker < out.find("# Marsh Lights").unwrap(), "{out}");
    // The last page's block goes to the very end; an empty anchor to the top of its page.
    let out = splice(
        RUN,
        &run_pages(),
        &[inset(5, "missing", "Tail block"), inset(5, "", "Top block")],
    );
    assert!(out.trim_end().ends_with("Tail block"), "{out}");
    let top = out.find("Top block").unwrap();
    assert!(top > out.find("They hum at dusk.").unwrap());
    assert!(top < out.find("# Marsh Lights").unwrap(), "{out}");
}

#[test]
fn several_insets_keep_their_order_and_the_layer_text() {
    let out = splice(
        RUN,
        &run_pages(),
        &[
            inset(5, "Lights drift over the reeds.", "First picture"),
            inset(5, "Lights drift over the reeds.", "Second picture"),
        ],
    );
    assert!(out.find("First picture").unwrap() < out.find("Second picture").unwrap());
    assert!(out.find("Second picture").unwrap() < out.find("A second paragraph").unwrap());
    for line in RUN.lines().filter(|l| !l.is_empty()) {
        assert!(out.contains(line), "{line}");
    }
    assert_eq!(splice(RUN, &run_pages(), &[]), RUN);
}

#[test]
fn a_block_that_repeats_the_text_layer_is_an_echo() {
    let layer = "Gloomwings roost under old bridges and hum at dusk.";
    assert!(echoes("Gloomwings roost under bridges at dusk.", layer));
    assert!(!echoes(
        "Armor Class 13, Hit Points 9, Speed 10 ft., fly 40 ft.",
        layer
    ));
    // Too few words to judge.
    assert!(!echoes("Gloomwings", layer));
}

#[test]
fn the_pictures_prompt_asks_for_verbatim_text_with_printed_names() {
    let part = PicturePart {
        label: pages_label("bestiary.pdf", &[3, 7]),
        pages: vec![(3, "Layer of page three".into())],
        attachment: Attachment {
            kind: AttachmentKind::Pdf,
            bytes: Vec::new(),
            name: "x".into(),
        },
    };
    assert_eq!(part.label, "the pictures on pages 3, 7 of `bestiary.pdf`");
    let step = pictures_step(&part);
    for phrase in [
        "VERBATIM",
        "each variant's own name as printed",
        "never merged",
        "--- page 3 ---\nLayer of page three",
        "pictures that print no text",
    ] {
        assert!(step.user.contains(phrase), "{phrase}");
    }
}

/// Offline measurement (no model calls): `BOREALIS_PICTURES_PROBE=<a.pdf>[,<b.pdf>]` prints, per
/// file, its text-layer pages and how many of them show a picture (one call per
/// `VISUAL_PAGES_PER_CALL` of those). Counts only.
#[test]
#[ignore = "needs local PDFs"]
fn probe_pictured_pages() {
    let Ok(files) = std::env::var("BOREALIS_PICTURES_PROBE") else {
        return;
    };
    for (i, f) in files.split(',').enumerate() {
        let bytes = std::fs::read(f).unwrap();
        let layout = crate::import::read_pdf_layout(&bytes).unwrap();
        let text: BTreeSet<u32> = layout
            .pages
            .iter()
            .filter(|p| p.has_text())
            .map(|p| p.number)
            .collect();
        let found = pictured_pages(&bytes, &text);
        println!(
            "file {i}: pages {} | text pages {} | pictured {} {:?}",
            layout.pages.len(),
            text.len(),
            found.len(),
            found
        );
    }
}
