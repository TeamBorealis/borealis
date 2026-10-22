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
        name: format!("Zq{kind}{}", char::from(b'a' + u8::try_from(i % 26).unwrap())),
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
        for want in ["Synthetic Hound", "Statistics", "Skills", "Traits", "Actions", "Passive Perception"] {
            assert!(text.contains(want), "{label}: missing {want}");
        }
        assert!(!all_text(&without).contains("Passive Perception"), "{label}");
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
    let html = render_html_with(&c, &s, &SheetOptions::default(), &content(Some(hound(1))))
        .expect("html");
    assert!(html.contains("Synthetic Hound"));
    assert!(html.contains("companion.stats"));
    let none = render_html_with(&c, &s, &SheetOptions::default(), &content(None)).expect("html");
    assert!(!none.contains("companion.stats"));
}
