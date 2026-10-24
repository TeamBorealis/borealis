//! The companion stat block gets its own page in the PDF and the HTML preview (S6), in both
//! editions, and nothing is dropped when the stat block is long.

mod common;

use borealis_sheet::{
    render_html_with, render_pdf_with, CompanionEntry, CompanionSheet, CompanionSkillLine,
    SheetContent, SheetOptions,
};
use lopdf::Document;

fn hound(entries: usize) -> CompanionSheet {
    let entry = |kind: &str, i: usize| CompanionEntry {
        name: format!(
            "Zq{kind}{}",
            char::from(b'a' + u8::try_from(i % 26).unwrap())
        ),
        text: "Does something useful on its turn, repeatedly, so that the text wraps. ".repeat(6),
    };
    CompanionSheet {
        creature: "Synthetic Hound".into(),
        name: "Zqname".into(),
        build: "Medium beast, unaligned".into(),
        armor_class: "13".into(),
        hit_points: "11".into(),
        initiative: 2,
        proficiency: 2,
        challenge: "1/4".into(),
        speed: 40,
        other_speeds: [0, 30, 0, 0],
        abilities: [12, 15, 12, 3, 12, 6],
        skills: vec![CompanionSkillLine {
            name: "Perception".into(),
            ability: "WIS".into(),
            bonus: 3,
        }],
        passive_perception: 13,
        traits: (0..entries).map(|i| entry("trait", i)).collect(),
        actions: (0..entries).map(|i| entry("act", i)).collect(),
        reactions: Vec::new(),
    }
}

fn flat() -> SheetOptions {
    SheetOptions {
        form_fillable: false,
        ..SheetOptions::default()
    }
}

fn all_text(pdf: &[u8]) -> String {
    let doc = Document::load_mem(pdf).expect("parses");
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    pages
        .iter()
        .filter_map(|p| doc.extract_text(&[*p]).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

fn content(companion: Option<CompanionSheet>) -> SheetContent {
    SheetContent {
        companion,
        ..SheetContent::default()
    }
}

#[test]
fn companion_page_prints_the_stat_block_in_both_editions() {
    for (label, (c, s)) in [
        ("2014", common::wizard_2014()),
        ("2024", common::wizard_2024()),
    ] {
        let without = render_pdf_with(&c, &s, &flat(), &content(None)).expect("pdf");
        let with = render_pdf_with(&c, &s, &flat(), &content(Some(hound(2)))).expect("pdf");
        let pages = |b: &[u8]| Document::load_mem(b).unwrap().get_pages().len();
        assert!(pages(&with) > pages(&without), "{label}: no companion page");
        let text = all_text(&with);
        for want in [
            "Synthetic Hound",
            "Statistics",
            "Skills",
            "Traits",
            "Actions",
            "Passive Perception",
            "Strength",
            "Charisma",
        ] {
            assert!(text.contains(want), "{label}: missing {want}");
        }
        assert!(
            !text.contains("STR 12 ("),
            "{label}: scores still printed as text"
        );
        if let Ok(dir) = std::env::var("BOREALIS_E2C_OUT") {
            std::fs::write(format!("{dir}/companion-{label}.pdf"), &with).unwrap();
        }
        assert!(
            !all_text(&without).contains("Passive Perception"),
            "{label}"
        );
    }
}

#[test]
fn long_companion_continues_on_more_pages_without_dropping_entries() {
    let (c, s) = common::wizard_2024();
    let sheet = hound(14);
    let pdf = render_pdf_with(&c, &s, &flat(), &content(Some(sheet.clone()))).expect("pdf");
    let text = all_text(&pdf);
    for e in sheet.traits.iter().chain(&sheet.actions) {
        assert!(text.contains(&e.name), "dropped {}", e.name);
    }
}

#[test]
fn html_preview_has_a_companion_page() {
    let (c, s) = common::wizard_2024();
    let html =
        render_html_with(&c, &s, &SheetOptions::default(), &content(Some(hound(1)))).expect("html");
    assert!(html.contains("Synthetic Hound"));
    assert!(html.contains("companion.stats"));
    // The scores are ability cards (sigil, modifier, score), not text lines.
    assert!(html.contains("data-field=\"companion.dex.score\""));
    assert!(html.contains("data-field=\"companion.cha.mod\""));
    let none = render_html_with(&c, &s, &SheetOptions::default(), &content(None)).expect("html");
    assert!(!none.contains("companion.stats"));
}

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for ch in data.chunks(3) {
        let n = ch
            .iter()
            .enumerate()
            .fold(0u32, |a, (i, b)| a | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= ch.len() {
                out.push(char::from(B64_TABLE[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A 40x50 PNG, generated here.
fn synthetic_png() -> Vec<u8> {
    let (w, h) = (40u32, 50u32);
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let px: Vec<u8> = (0..w * h)
        .flat_map(|i| [u8::try_from(i % 251).unwrap(), 90, 160])
        .collect();
    writer.write_image_data(&px).unwrap();
    drop(writer);
    out
}

fn with_portrait(companion: Option<CompanionSheet>, bytes: &[u8]) -> SheetContent {
    SheetContent {
        companion,
        companion_portrait: Some(borealis_character::Portrait {
            file_name: "companion.png".into(),
            data_base64: base64_encode(bytes),
        }),
        ..SheetContent::default()
    }
}

/// Pages (1-based) whose content draws an image XObject, and the page count.
fn image_pages(pdf: &[u8]) -> (Vec<u32>, usize) {
    use lopdf::Object;
    let doc = Document::load_mem(pdf).expect("parses");
    let pages = doc.get_pages();
    let is_image = |id: lopdf::ObjectId| {
        doc.get_object(id)
            .and_then(Object::as_stream)
            .is_ok_and(|s| {
                s.dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|n| n == b"Image")
            })
    };
    let mut drawing = Vec::new();
    for (n, id) in &pages {
        let (res, _) = doc.get_page_resources(*id).expect("resources");
        let names: Vec<String> = res
            .and_then(|r| r.get(b"XObject").ok())
            .and_then(|x| match x {
                Object::Dictionary(d) => Some(d),
                Object::Reference(r) => doc.get_dictionary(*r).ok(),
                _ => None,
            })
            .map(|d| {
                d.iter()
                    .filter(|(_, v)| v.as_reference().is_ok_and(is_image))
                    .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let content = String::from_utf8_lossy(&doc.get_page_content(*id)).into_owned();
        if names
            .iter()
            .any(|name| content.contains(&format!("/{name} Do")))
        {
            drawing.push(*n);
        }
    }
    (drawing, pages.len())
}

#[test]
fn companion_portrait_is_embedded_on_the_companion_page() {
    let (c, s) = common::wizard_2024();
    let plain = render_pdf_with(&c, &s, &flat(), &content(Some(hound(1)))).expect("pdf");
    let content = with_portrait(Some(hound(1)), &synthetic_png());
    let pdf = render_pdf_with(&c, &s, &flat(), &content).expect("pdf");
    let (plain_images, plain_pages) = image_pages(&plain);
    let (images, pages) = image_pages(&pdf);
    assert!(plain_images.is_empty(), "the wizard has no other image");
    assert_eq!(pages, plain_pages, "the portrait box adds no page");
    // The companion page is the last page of this sheet (no cards selected).
    assert_eq!(images, vec![u32::try_from(pages).unwrap()]);
    assert!(all_text(&pdf).contains("Synthetic Hound"));
    if let Ok(dir) = std::env::var("BOREALIS_E2C_OUT") {
        std::fs::write(format!("{dir}/companion-portrait.pdf"), &pdf).unwrap();
        let html = render_html_with(&c, &s, &SheetOptions::default(), &content).expect("html");
        std::fs::write(format!("{dir}/companion-portrait.html"), html).unwrap();
    }
}

/// Every `/Alt` text in the document's structure tree.
fn alt_texts(pdf: &[u8]) -> Vec<String> {
    let doc = Document::load_mem(pdf).expect("parses");
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter_map(|d| d.get(b"Alt").ok())
        .filter_map(|a| a.as_str().ok())
        .map(|b| {
            let units: Vec<u16> = b
                .strip_prefix(&[0xFE, 0xFF])
                .unwrap_or(b)
                .chunks(2)
                .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                .collect();
            String::from_utf16_lossy(&units)
        })
        .collect()
}

#[test]
fn tagged_companion_portrait_has_its_own_alt_text() {
    let (c, s) = common::wizard_2024();
    let content = with_portrait(Some(hound(1)), &synthetic_png());
    let pdf = render_pdf_with(&c, &s, &flat(), &content).expect("pdf");
    let alts = alt_texts(&pdf);
    assert!(alts.iter().any(|a| a == "Companion portrait"), "{alts:?}");
    assert!(!alts.iter().any(|a| a == "Character portrait"), "{alts:?}");
}

#[test]
fn companion_without_portrait_embeds_nothing_and_is_unchanged() {
    let (c, s) = common::wizard_2024();
    let a = render_pdf_with(&c, &s, &flat(), &content(Some(hound(1)))).expect("pdf");
    assert!(image_pages(&a).0.is_empty());
    let html =
        render_html_with(&c, &s, &SheetOptions::default(), &content(Some(hound(1)))).expect("html");
    assert!(!html.contains("class=\"portrait companion-portrait"));
    assert!(!html.contains("Companion portrait"));
    // The only difference a portrait makes is its box: the stat-block text is the same.
    let with = render_pdf_with(
        &c,
        &s,
        &flat(),
        &with_portrait(Some(hound(1)), &synthetic_png()),
    )
    .expect("pdf");
    assert_eq!(all_text(&a), all_text(&with));
}

#[test]
fn portrait_alone_adds_no_companion_page() {
    for (label, (c, s)) in [
        ("2014", common::wizard_2014()),
        ("2024", common::wizard_2024()),
    ] {
        let without = render_pdf_with(&c, &s, &flat(), &content(None)).expect("pdf");
        let only = with_portrait(None, &synthetic_png());
        let pdf = render_pdf_with(&c, &s, &flat(), &only).expect("pdf");
        let (images, pages) = image_pages(&pdf);
        assert_eq!(pages, image_pages(&without).1, "{label}: no extra page");
        assert!(images.is_empty(), "{label}: nothing embedded");

        let html = render_html_with(&c, &s, &SheetOptions::default(), &only).expect("html");
        assert!(!html.contains("Companion portrait"), "{label}");
    }
}

#[test]
fn unreadable_companion_portrait_leaves_an_empty_box() {
    let (c, s) = common::wizard_2024();
    let junk = with_portrait(Some(hound(1)), b"not an image at all");
    let pdf = render_pdf_with(&c, &s, &flat(), &junk).expect("pdf still renders");
    assert!(image_pages(&pdf).0.is_empty());
    let html = render_html_with(&c, &s, &SheetOptions::default(), &junk).expect("html");
    assert!(!html.contains("<img class=\"portrait companion-portrait\""));
}

#[test]
fn html_companion_portrait_sits_in_its_box_on_the_companion_page() {
    let (c, s) = common::wizard_2024();
    let content = with_portrait(Some(hound(1)), &synthetic_png());
    let html = render_html_with(&c, &s, &SheetOptions::default(), &content).expect("html");
    let page = html
        .find("data-page=\"companion\"")
        .expect("companion page present");
    let img = html.find("Companion portrait").expect("portrait embedded");
    assert!(page < img, "portrait is on the companion page");
    assert!(!html[..page].contains("Companion portrait"));
    // The six score cards are siblings of the grid, not nested in each other.
    let companion = &html[page..];
    assert_eq!(
        companion.matches("<div class=\"ab\">").count(),
        6,
        "six score cards"
    );
    assert!(companion.contains("</div></div><div class=\"ab\">"));
}
