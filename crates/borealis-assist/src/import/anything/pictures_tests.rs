use std::collections::BTreeSet;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

use super::*;

/// Text at (x, y, size).
pub(crate) type Line<'a> = (f32, f32, f32, &'a str);

/// A PDF of letter pages, each with its text and an optional picture `[x, y, width, height]`
/// (a 2 x 2 image: too few pixels to judge, so only its place and size count).
pub(crate) fn pdf(pages: &[(Vec<Line<'_>>, Option<[f32; 4]>)]) -> Vec<u8> {
    let image = Stream::new(
        dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
        "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8},
        vec![200u8; 12],
    )
    .with_compression(false);
    pdf_with(pages, image)
}

/// A gray image of `w` x `h` pixels, Flate-compressed.
fn gray_image(w: usize, h: usize, px: &[u8]) -> Stream {
    let mut s = Stream::new(
        dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => w as i64,
        "Height" => h as i64, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8},
        px.to_vec(),
    );
    s.compress().unwrap();
    s
}

/// Pages as [`pdf`], every picture painting `image`.
fn pdf_with(pages: &[(Vec<Line<'_>>, Option<[f32; 4]>)], image: Stream) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(
        dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"},
    );
    let img = doc.add_object(image);
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

pub(crate) fn lore<'a>() -> Vec<Line<'a>> {
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
        probe_boxes(&bytes, &text);
    }
}

/// One line per image that may be a picture: page, box, its pixels' verdict, decoration.
fn probe_boxes(bytes: &[u8], text: &BTreeSet<u32>) {
    let doc = Document::load_mem(bytes).unwrap();
    let mut cache = FontCache::default();
    let mut budget = WALK_BUDGET;
    let mut memo = std::collections::HashMap::new();
    for (n, id) in doc.get_pages() {
        if !text.contains(&n) {
            continue;
        }
        let painted = paint_page(&doc, id, &mut cache, &mut budget);
        let size = page_size(&doc, id);
        let images = page_images(&doc, id);
        for b in painted
            .images
            .iter()
            .filter(|b| may_be_picture(b, &painted.runs, size))
        {
            let ink = ink_of_box(&doc, b, &images, &mut memo);
            let near = b.x0 < 18.0 || b.y0 < 18.0 || b.x1 > size.0 - 18.0 || b.y1 > size.1 - 18.0;
            println!(
                "  p{n} box {:.0},{:.0} {:.0}x{:.0} share {:.2} edge {near} ink {ink:?} decoration {}",
                b.x0,
                b.y0,
                b.width(),
                b.height(),
                b.width() * b.height() / (size.0 * size.1),
                is_decoration(b, &painted.runs, size, n == 1, ink)
            );
        }
    }
}

/// Pixels of a pasted text picture: `lines` lines of dark word marks on a light ground, with
/// a portrait-like dark block over the left `portrait` share of the width.
fn text_pixels(w: usize, h: usize, lines: usize, portrait: f32) -> Vec<u8> {
    let mut px = vec![235u8; w * h];
    let pitch = h / (lines + 1);
    for line in 0..lines {
        let top = pitch / 2 + line * pitch;
        for y in top..top + pitch / 2 {
            for x in 4..w - 4 {
                // Words of 9 marks, a space of 3.
                if x % 12 < 9 && x % 2 == 0 {
                    px[y * w + x] = 30;
                }
            }
        }
    }
    let art = (w as f32 * portrait) as usize;
    for y in 0..h {
        for x in 0..art {
            px[y * w + x] = ((x * 7 + y * 13) % 200) as u8;
        }
    }
    px
}

/// Pixels of a table: a dark header band and rows of cells split by rules.
fn table_pixels(w: usize, h: usize, rows: usize) -> Vec<u8> {
    let mut px = text_pixels(w, h, rows, 0.0);
    for y in 0..h / (rows + 1) / 2 {
        for x in 0..w {
            px[y * w + x] = 60;
        }
    }
    px
}

/// Pixels of painted art: tones everywhere, no flat ground.
fn art_pixels(w: usize, h: usize) -> Vec<u8> {
    let mut seed: u32 = 7;
    (0..w * h)
        .map(|i| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let shade = (i % w + i / w) * 255 / (w + h);
            ((shade as u32 + (seed >> 24) % 90) % 256) as u8
        })
        .collect()
}

fn judged(w: usize, h: usize, px: Vec<u8>) -> Ink {
    picture_ink::judge(&picture_ink::Pixels {
        width: w,
        height: h,
        gray: px,
        alpha: None,
    })
}

#[test]
fn pixels_tell_printed_lines_from_art() {
    assert_eq!(judged(300, 400, text_pixels(300, 400, 14, 0.0)), Ink::Text);
    // A portrait beside the stat block fills every row, not every strip.
    assert_eq!(judged(400, 300, text_pixels(400, 300, 10, 0.4)), Ink::Text);
    assert_eq!(judged(400, 160, table_pixels(400, 160, 4)), Ink::Text);
    assert_eq!(judged(300, 300, art_pixels(300, 300)), Ink::NoText);
    // A logo word: one line of marks.
    assert_eq!(judged(300, 100, text_pixels(300, 100, 1, 0.0)), Ink::NoText);
    // Too few pixels to say.
    assert_eq!(judged(8, 8, vec![0; 64]), Ink::Unknown);
}

#[test]
fn transparent_pixels_are_ground() {
    // Cut-out art: opaque tones inside a transparent frame, no lines of text.
    let (w, h) = (200, 200);
    let alpha: Vec<u8> = (0..w * h)
        .map(|i| {
            if (40..160).contains(&(i % w)) && (30..170).contains(&(i / w)) {
                255
            } else {
                0
            }
        })
        .collect();
    let p = picture_ink::Pixels {
        width: w,
        height: h,
        gray: art_pixels(w, h),
        alpha: Some(alpha),
    };
    assert_eq!(picture_ink::judge(&p), Ink::NoText);
}

#[test]
fn pasted_stat_blocks_and_tables_are_read_wherever_they_sit() {
    let (w, h) = (600, 480);
    let block = || gray_image(w, h, &text_pixels(w, h, 16, 0.0));
    // Full width, touching both side edges and the bottom edge.
    let full = pdf_with(&[(lore(), Some([0.0, 0.0, 612.0, 490.0]))], block());
    assert_eq!(pictured_pages(&full, &all_pages(1)), BTreeSet::from([1]));
    // Bleeding off the right edge.
    let bleeding = pdf_with(&[(lore(), Some([330.0, 120.0, 290.0, 232.0]))], block());
    assert_eq!(
        pictured_pages(&bleeding, &all_pages(1)),
        BTreeSet::from([1])
    );
    // A wide table along the bottom edge.
    let (tw, th) = (600, 150);
    let table = gray_image(tw, th, &table_pixels(tw, th, 4));
    let tabled = pdf_with(&[(lore(), Some([6.0, 2.0, 600.0, 150.0]))], table);
    assert_eq!(pictured_pages(&tabled, &all_pages(1)), BTreeSet::from([1]));
    // Pixels that cannot be judged: the edge alone never makes it decoration.
    let unknown = pdf(&[(lore(), Some([0.0, 0.0, 612.0, 300.0]))]);
    assert_eq!(pictured_pages(&unknown, &all_pages(1)), BTreeSet::from([1]));
}

#[test]
fn art_and_a_title_pages_logo_are_decoration() {
    let title = vec![
        (200.0, 700.0, 24.0, "The Lantern Almanac"),
        (230.0, 120.0, 10.0, "by A. Writer"),
    ];
    let (w, h) = (300, 420);
    let art = || gray_image(w, h, &art_pixels(w, h));
    // Art bleeding off the right and bottom edges of a text page, and the same art inside.
    let bytes = pdf_with(
        &[
            (lore(), Some([420.0, -6.0, 200.0, 280.0])),
            (lore(), Some([150.0, 200.0, 200.0, 280.0])),
        ],
        art(),
    );
    assert!(pictured_pages(&bytes, &all_pages(2)).is_empty());
    // A small emblem centred on a file's title page; the same emblem on a later page is read.
    let emblem = pdf(&[
        (title.clone(), Some([246.0, 330.0, 120.0, 190.0])),
        (title.clone(), Some([246.0, 330.0, 120.0, 190.0])),
    ]);
    assert_eq!(pictured_pages(&emblem, &all_pages(2)), BTreeSet::from([2]));
    // A small pasted stat block centred on a title page shows its lines: read.
    let (sw, sh) = (240, 380);
    let small = gray_image(sw, sh, &text_pixels(sw, sh, 18, 0.0));
    let bytes = pdf_with(&[(title, Some([246.0, 330.0, 120.0, 190.0]))], small);
    assert_eq!(pictured_pages(&bytes, &all_pages(1)), BTreeSet::from([1]));
    // A second look at a page takes decoration too.
    let bytes = pdf_with(&[(lore(), Some([150.0, 200.0, 200.0, 280.0]))], art());
    assert_eq!(imaged_pages(&bytes, &all_pages(1)), BTreeSet::from([1]));
}
