//! Tests of the features, gear and story pages, and the visual sample writer.

use super::build_with_limits;
use super::features_text::wrap_field;
use crate::fonts::text_width;
use crate::layout::grid::Grid;
use crate::layout::page_story;
use crate::layout::{chrome, LayoutOptions};
use crate::model_fixtures::{
    cleric_2014, table_feature, table_feature_with, with_sample_extras, wizard_2014, wizard_2024,
};
use crate::model_types::{
    Edition, FeatureData, FeatureGroup, GearItem, SheetData, SpellFactsMap, StoryData,
};
use crate::pdf::{write_document, FormMode, PageOp, PdfDocument, PdfPage};
use crate::pdf_draw::{Paint, Rgb};
use crate::pdf_form::FieldKind;
use crate::Paper;
use std::collections::{HashMap, HashSet};

type Pair = (
    borealis_character::Character,
    borealis_engine::CharacterSnapshot,
);

fn data(p: &Pair) -> SheetData {
    crate::sheet_data(&p.0, &p.1, &SpellFactsMap::new())
}

fn story() -> StoryData {
    let s = |t: &str| t.to_string();
    StoryData {
        gender: s("Female"),
        age: s("27"),
        height: s("5'6\""),
        weight: s("140 lb"),
        eyes: s("Grey"),
        skin: s("Fair"),
        hair: s("Auburn"),
        has_portrait: false,
        traits: s("I quote sacred texts at odd moments.\nI always have a plan."),
        ideals: s("The forge-fires must never go out."),
        bonds: s("The temple that raised me."),
        flaws: s("I trust iron more than people."),
        trinket: s("A cracked holy symbol."),
        backstory: s("Raised by the keeper of a lighthouse.\n\nShe left at sixteen."),
        organization: s("Order of the Lantern"),
        allies: s("The harbourmaster."),
        treasure: s("A sapphire the size of a thumbnail."),
        quest: s("The sealed letter."),
        equipment_notes: s("Spare rope in the cellar."),
        additional_features: s("Lighthouse keeper's luck."),
        notes: s("Check the harbour ledger."),
    }
}

fn with_story(mut d: SheetData) -> SheetData {
    d.story = story();
    d.identity.deity = "Pelor".to_string();
    d
}

fn fixtures() -> Vec<SheetData> {
    vec![
        with_story(data(&wizard_2014())),
        with_story(data(&wizard_2024())),
        with_story(data(&cleric_2014())),
    ]
}

/// A character with `n_feats` long features and `n_items` items.
fn overflow(mut d: SheetData, n_feats: usize, n_items: usize) -> SheetData {
    for i in 0..n_feats {
        let para = "The creature must make a saving throw against the effect or be moved \
                    thirty feet in a direction of your choice, and you learn its name.";
        let long = i % 3 == 0;
        let text = if long {
            format!("{para}\n{para}\n{para}\n{para}\n{para} Feature {i} ends here.")
        } else {
            format!("Short feature {i}. {para}")
        };
        d.features.push(FeatureData {
            id: format!("ID_FEATURE_{i}"),
            name: format!("Synthetic feature number {i}"),
            group: [
                FeatureGroup::Class,
                FeatureGroup::Species,
                FeatureGroup::Feat,
                FeatureGroup::Other,
            ][i % 4],
            class: None,
            level: (i % 5 + 1) as u32,
            sources: Vec::new(),
            summary: format!("Feature {i} in short."),
            tables: Vec::new(),
            text,
        });
    }
    for i in 0..n_items {
        d.gear.items.push(GearItem {
            name: format!("Odd item number {i}"),
            quantity: (i % 7 + 1) as u32,
            weight: i as f64 / 4.0,
            equipped: if i % 5 == 0 {
                "worn".into()
            } else {
                String::new()
            },
            attuned: false,
            requires_attunement: false,
        });
    }
    d.story.treasure = (0..70)
        .map(|i| format!("Treasure line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    d
}

fn pages_of(d: &SheetData, o: &LayoutOptions) -> (Vec<PdfPage>, Vec<Vec<(usize, f32)>>) {
    let g = Grid::new(o.paper);
    let (f, mut limits) = build_with_limits(d, o, &g, 2, None);
    let (st, st_limits) = page_story::build_with_limits(d, o, &g, 3);
    let mut pages = f.all();
    pages.extend(st);
    limits.extend(st_limits);
    (pages, limits)
}

fn field_names(pages: &[PdfPage]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter_map(|o| match o {
            // The chrome header repeats page 1's `char.*` fields on every page (the writer
            // renames the repeats); only the deity field belongs to these pages.
            PageOp::Field(f)
                if !f.name.starts_with("char.") || f.name.starts_with("char.deity") =>
            {
                Some(f.name.clone())
            }
            _ => None,
        })
        .collect()
}

fn base_id(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

/// Whether a sheet id belongs to the features/gear/story family for the edition.
fn belongs(id: &str, edition: Edition, story_page: bool) -> bool {
    let gear = ["feat.", "equip.", "attune", "coin.", "carry."];
    if gear.iter().any(|p| id.starts_with(p)) || id == "char.deity" {
        return true;
    }
    if id.starts_with("story.") || id == "xtra.companion" {
        if edition == Edition::Dnd2014 {
            return true;
        }
        let on_page_2 = [
            "story.treasure",
            "story.quest",
            "story.equipment_notes",
            "story.additional_features",
        ];
        return on_page_2.contains(&id) || story_page;
    }
    false
}

fn check_ids(d: &SheetData, pages: &[PdfPage]) {
    let names = field_names(pages);
    let unique: HashSet<&String> = names.iter().collect();
    let mut seen = HashSet::new();
    let dups: Vec<&String> = names.iter().filter(|n| !seen.insert(*n)).collect();
    assert!(dups.is_empty(), "duplicate full field names: {dups:?}");
    let _ = unique;
    let known: Vec<String> = d.fields().into_iter().map(|f| f.id).collect();
    let mut count: HashMap<&str, usize> = HashMap::new();
    for n in &names {
        *count.entry(base_id(n)).or_default() += 1;
    }
    let story_page = crate::layout::has_story_page(d);
    for n in &names {
        assert!(known.iter().any(|k| k == base_id(n)), "invented id {n}");
    }
    for id in &known {
        if !belongs(id, d.edition, story_page) {
            continue;
        }
        let c = count.get(id.as_str()).copied().unwrap_or(0);
        // A summarised feature has its summary on the main page and the full text on details pages.
        let ok = if id.starts_with("feat.") && id.ends_with(".text") {
            c >= 1
        } else {
            c >= 1 && (c == 1 || id.starts_with("story.") || id.starts_with("equip."))
        };
        assert!(ok, "{id} appears {c} times ({:?})", d.edition);
    }
}

#[test]
fn every_id_of_these_pages_appears_for_both_editions() {
    for d in fixtures() {
        let (pages, _) = pages_of(&d, &LayoutOptions::default());
        check_ids(&d, &pages);
    }
}

#[test]
fn ids_present_without_a_story_page_for_2024() {
    let mut d = data(&wizard_2024());
    d.story = StoryData::default();
    let o = LayoutOptions::default();
    let g = Grid::new(o.paper);
    assert!(page_story::build(&d, &o, &g, 3).is_empty());
    let (pages, _) = pages_of(&d, &o);
    check_ids(&d, &pages);
}

#[test]
fn story_page_absent_for_2014_and_for_empty_story() {
    let o = LayoutOptions::default();
    let g = Grid::new(o.paper);
    let d14 = with_story(data(&wizard_2014()));
    assert!(page_story::build(&d14, &o, &g, 3).is_empty());
    let mut d24 = data(&wizard_2024());
    d24.story = StoryData::default();
    assert!(page_story::build(&d24, &o, &g, 3).is_empty());
    assert!(!page_story::build(&with_story(data(&wizard_2024())), &o, &g, 3).is_empty());
}

fn check_limits(pages: &[PdfPage], limits: &[Vec<(usize, f32)>]) {
    for (page, lim) in pages.iter().zip(limits) {
        for (i, limit) in lim {
            if let PageOp::Text {
                style, size, text, ..
            } = &page.ops[*i]
            {
                let w = text_width(*style, text, *size);
                assert!(w <= limit + 0.05, "{text:?} is {w} wide, limit {limit}");
            }
        }
        for op in &page.ops {
            let PageOp::Field(f) = op else { continue };
            let FieldKind::Text(t) = &f.kind else {
                continue;
            };
            if t.multiline {
                let n = wrap_field(t.style, t.size, f.w - 2.0 * t.pad, &t.value).len();
                let lead = t.leading.unwrap_or(t.size * 1.25);
                assert!(
                    n as f32 * lead <= f.h + 0.6,
                    "{} clips: {n} lines in {}",
                    f.name,
                    f.h
                );
            } else {
                let w = text_width(t.style, &t.value, t.size);
                assert!(
                    w <= f.w - 2.0 * t.pad + 0.05 || t.size <= 4.81,
                    "{} {:?}",
                    f.name,
                    t.value
                );
            }
        }
    }
}

#[test]
fn nothing_exceeds_its_box() {
    let mut sets = fixtures();
    sets.push(overflow(with_story(data(&wizard_2024())), 40, 60));
    sets.push(overflow(with_story(data(&wizard_2014())), 40, 60));
    for d in sets {
        for paper in [Paper::Letter, Paper::A4] {
            for compact in [false, true] {
                let o = LayoutOptions {
                    paper,
                    compact,
                    ..LayoutOptions::default()
                };
                let (pages, limits) = pages_of(&d, &o);
                check_limits(&pages, &limits);
            }
        }
    }
}

fn words(s: &str) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for w in s.split_whitespace() {
        *m.entry(w.to_string()).or_default() += 1;
    }
    m
}

fn contains_all(whole: &str, part: &str) -> bool {
    let w = words(whole);
    words(part)
        .iter()
        .all(|(k, n)| w.get(k).copied().unwrap_or(0) >= *n)
}

#[test]
fn forty_features_and_sixty_items_continue_and_drop_nothing() {
    for base in [data(&wizard_2024()), data(&wizard_2014())] {
        let d = overflow(with_story(base), 40, 60);
        let o = LayoutOptions::default();
        let (pages, _) = pages_of(&d, &o);
        assert!(
            pages.len() > 3,
            "expected continuation pages, got {}",
            pages.len()
        );
        check_ids(&d, &pages);
        // Every feature's full text and every item name is somewhere in a field.
        let mut texts: HashMap<String, String> = HashMap::new();
        let mut all = String::new();
        for op in pages.iter().flat_map(|p| &p.ops) {
            if let PageOp::Field(f) = op {
                if let FieldKind::Text(t) = &f.kind {
                    texts
                        .entry(base_id(&f.name).to_string())
                        .or_default()
                        .push_str(&format!(" {}", t.value));
                    all.push_str(&format!(" {}\n", t.value));
                }
            }
        }
        for (i, f) in d.features.iter().enumerate() {
            let joined = texts
                .get(&format!("feat.{}.text", i + 1))
                .cloned()
                .unwrap_or_default();
            assert!(
                contains_all(&joined, &f.text),
                "feature {} lost text",
                f.name
            );
        }
        for it in &d.gear.items {
            assert!(all.contains(&it.name), "{} dropped", it.name);
        }
        assert!(
            contains_all(&texts["story.treasure"], &d.story.treasure),
            "treasure lost"
        );
        let titles: Vec<&str> = pages
            .iter()
            .flat_map(|p| &p.ops)
            .filter_map(|o| match o {
                PageOp::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(titles.iter().any(|t| t.contains("(continued)")));
        if d.edition == Edition::Dnd2024 {
            assert!(titles.contains(&"Feature details"), "details page missing");
        }
    }
}

#[test]
fn split_blocks_keep_their_heading_with_text() {
    // Every feature name field is followed (same page) by a text field with the same index.
    let d = overflow(with_story(data(&wizard_2024())), 40, 10);
    let (pages, _) = pages_of(&d, &LayoutOptions::default());
    for p in &pages {
        let names = field_names(std::slice::from_ref(p));
        for (i, n) in names.iter().enumerate() {
            if let Some(rest) = n.strip_prefix("feat.") {
                if let Some(idx) = rest.split('.').next() {
                    if rest.contains(".name") {
                        let want = format!("feat.{idx}.text");
                        assert!(
                            names[i + 1..].iter().any(|m| base_id(m) == want),
                            "{n} orphaned on its page"
                        );
                    }
                }
            }
        }
    }
}

fn is_grey(c: Rgb) -> bool {
    (c.0 - c.1).abs() < 0.002 && (c.1 - c.2).abs() < 0.002
}

fn paint_grey(p: &Paint) -> bool {
    p.fill.is_none_or(is_grey) && p.stroke.is_none_or(is_grey)
}

#[test]
fn ink_mode_is_grey_only() {
    for d in fixtures() {
        let o = LayoutOptions {
            ink: true,
            ..LayoutOptions::default()
        };
        let (pages, _) = pages_of(&overflow(d, 12, 20), &o);
        for op in pages.iter().flat_map(|p| &p.ops) {
            let ok = match op {
                PageOp::Text { color, .. } => is_grey(*color),
                PageOp::Rect { fill, stroke, .. } => {
                    fill.is_none_or(is_grey) && stroke.is_none_or(is_grey)
                }
                PageOp::Line { color, .. } => is_grey(*color),
                PageOp::Path { paint, .. } => paint_grey(paint),
                PageOp::Field(f) => {
                    f.frame.is_none_or(|fr| paint_grey(&fr.paint))
                        && match &f.kind {
                            FieldKind::Text(t) => is_grey(t.color),
                            FieldKind::Check(c) => is_grey(c.color),
                        }
                }
                _ => true,
            };
            assert!(ok, "non-grey colour in ink mode: {op:?}");
        }
    }
}

fn document(pages: Vec<PdfPage>, d: &SheetData, o: &LayoutOptions) -> PdfDocument<'static> {
    let g = Grid::new(o.paper);
    let mut pages = pages;
    chrome::footers(&mut pages, &g, d, o.ink);
    PdfDocument {
        page_w: g.w,
        page_h: g.h,
        pages,
        images: Vec::new(),
        title: "E2a sample".to_string(),
        lang: Some(o.lang.clone()),
        mode: FormMode::Fillable,
        structure: None,
    }
}

#[test]
fn documents_serialise() {
    let d = overflow(with_story(data(&wizard_2024())), 40, 60);
    let o = LayoutOptions::default();
    let (pages, _) = pages_of(&d, &o);
    assert!(write_document(&document(pages, &d, &o)).starts_with(b"%PDF"));
}

/// Writes samples when `BOREALIS_E2A_SAMPLE` names an output directory (visual check).
#[test]
fn write_samples() {
    let Ok(dir) = std::env::var("BOREALIS_E2A_SAMPLE") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let sets = [
        ("2014", with_story(data(&cleric_2014()))),
        ("2024", with_story(data(&wizard_2024()))),
        ("2014-empty", data(&wizard_2014())),
        ("2024-empty", data(&wizard_2024())),
        (
            "2024-overflow",
            overflow(with_story(data(&wizard_2024())), 40, 60),
        ),
        (
            "2014-overflow",
            overflow(with_story(data(&wizard_2014())), 40, 60),
        ),
        ("2014-tables", {
            let mut d = with_sample_extras(data(&cleric_2014()));
            d.features
                .insert(1, table_feature(70, 4, "A longer value that wraps"));
            d
        }),
        ("2024-tables", with_sample_extras(data(&wizard_2024()))),
    ];
    for (name, d) in &sets {
        for (tag, ink, paper) in [
            ("colour-letter", false, Paper::Letter),
            ("ink-letter", true, Paper::Letter),
            ("colour-a4", false, Paper::A4),
            ("ink-a4", true, Paper::A4),
        ] {
            let o = LayoutOptions {
                ink,
                paper,
                ..LayoutOptions::default()
            };
            let (pages, _) = pages_of(d, &o);
            std::fs::write(
                format!("{dir}/{name}-{tag}.pdf"),
                write_document(&document(pages, d, &o)),
            )
            .unwrap();
        }
    }
}

#[test]
fn weights_and_coins_group_thousands_like_xp() {
    use super::features_flow::{group_digits, trim_num};
    assert_eq!(trim_num(3724.56), "3,724.56");
    assert_eq!(trim_num(3.0), "3");
    assert_eq!(trim_num(2.5), "2.5");
    assert_eq!(trim_num(1_000_000.0), "1,000,000");
    assert_eq!(group_digits("123456"), "123,456");
    assert_eq!(group_digits("-1234"), "-1,234");
    assert_eq!(group_digits("999"), "999");
}

/// Lowest text line of a page, ignoring the footer.
fn lowest_text(p: &PdfPage, body_bottom: f32) -> f32 {
    p.ops
        .iter()
        .filter_map(|op| match op {
            PageOp::Text { y, .. } if *y < body_bottom => Some(*y),
            _ => None,
        })
        .fold(0.0, f32::max)
}

#[test]
fn feature_details_use_the_room_left_on_the_last_continuation_page() {
    // Only the last page of the sequence may end early; the full-text blocks used to start a page
    // of their own after a nearly empty continuation page.
    let g = Grid::new(Paper::Letter);
    for n_feats in [10, 18] {
        let d = overflow(with_story(data(&wizard_2024())), n_feats, 0);
        let (f, _) = build_with_limits(&d, &LayoutOptions::default(), &g, 2, None);
        let last = f.main.len() - 1;
        for (i, p) in f.main.iter().enumerate().take(last) {
            assert!(
                lowest_text(p, g.body_bottom) > g.body_bottom - 40.0,
                "page {i} of {n_feats} features ends early"
            );
        }
    }
}

#[test]
fn inventory_rows_go_on_in_the_right_column_before_a_new_page() {
    // M1 (2014): rows that overflowed the left column used to start a page of their own while
    // the right column had room below coins and carrying.
    let g = Grid::new(Paper::Letter);
    let o = LayoutOptions::default();
    for n in [40, 60, 90, 140] {
        let d = overflow(data(&wizard_2014()), 0, n);
        let (f, _) = build_with_limits(&d, &o, &g, 2, None);
        let inv = &f.inventory;
        if n <= 60 {
            assert_eq!(inv.len(), 1, "{n} items fit one page");
        }
        let names = field_names(inv);
        for i in 1..=n {
            let id = crate::model_ids::equip(i, "name");
            let c = names.iter().filter(|x| base_id(x) == id).count();
            assert_eq!(c, 1, "{id} of {n}");
        }
        let heads = |p: &PdfPage| -> usize {
            p.ops
                .iter()
                .filter(
                    |op| matches!(op, PageOp::Text { text, .. } if text == "Equipment (continued)"),
                )
                .count()
        };
        assert!(
            n <= 40 || heads(&inv[0]) == 1,
            "{n}: rows go on in the right column"
        );
        for (i, p) in inv.iter().enumerate().take(inv.len() - 1) {
            assert!(
                lowest_field(p, g.body_bottom) > g.body_bottom - 40.0,
                "inventory page {i} of {n} items ends early"
            );
            assert!(
                heads(&inv[i + 1]) >= 1,
                "the next page repeats the table heading"
            );
        }
    }
}

/// Bottom of the lowest form field of a page (equipment rows are fields), ignoring the footer.
fn lowest_field(p: &PdfPage, body_bottom: f32) -> f32 {
    p.ops
        .iter()
        .filter_map(|op| match op {
            PageOp::Field(f) if f.y + f.h <= body_bottom => Some(f.y + f.h),
            _ => None,
        })
        .fold(0.0, f32::max)
}

/// Whether a page holds nothing but empty write-in fields (the chrome header aside).
fn blank_write_ins_only(p: &PdfPage) -> bool {
    p.ops.iter().all(|op| match op {
        PageOp::Field(f) if f.name.starts_with("char.") && !f.name.starts_with("char.deity") => {
            true
        }
        PageOp::Field(f) => {
            let blank = matches!(&f.kind, FieldKind::Text(t) if t.value.trim().is_empty());
            blank && (f.name.starts_with("story.") || f.name.starts_with("xtra."))
        }
        _ => true,
    })
}

/// Highest "continued on page N" of a sequence.
fn last_continued_target(pages: &[PdfPage]) -> Option<u32> {
    pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter_map(|op| match op {
            PageOp::Text { text, .. } => text.strip_prefix("continued on page ")?.parse().ok(),
            _ => None,
        })
        .max()
}

/// `n` numbered lines.
fn numbered_lines(n: usize) -> String {
    (0..n)
        .map(|i| format!("Line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn no_trailing_page_of_blank_notes_only() {
    // B-21 M1 remainder: when the columns were exactly full, the empty Notes region (and other
    // blank write-ins) started a last Details / Story page of its own.
    for paper in [Paper::Letter, Paper::A4] {
        let o = LayoutOptions {
            paper,
            ..LayoutOptions::default()
        };
        let g = Grid::new(paper);
        for base in [data(&wizard_2014()), data(&wizard_2024())] {
            for lines in 0..120 {
                let mut d = with_story(base.clone());
                d.story.notes = String::new();
                d.story.quest = String::new();
                d.story.allies = String::new();
                if d.edition == Edition::Dnd2014 {
                    d.story.treasure = numbered_lines(lines);
                } else {
                    d.story.backstory = numbered_lines(lines);
                }
                let first = 2;
                let pages = if d.edition == Edition::Dnd2014 {
                    build_with_limits(&d, &o, &g, first, None).0.main
                } else {
                    page_story::build(&d, &o, &g, first)
                };
                for (i, p) in pages.iter().enumerate().skip(1) {
                    assert!(
                        !blank_write_ins_only(p),
                        "{:?} {paper:?} {lines} lines: page {i} holds only blank write-ins",
                        d.edition
                    );
                }
                let last = first + pages.len() as u32 - 1;
                if let Some(n) = last_continued_target(&pages) {
                    assert!(
                        n <= last,
                        "{lines} lines: 'continued on page {n}' of {last}"
                    );
                }
            }
        }
    }
}

fn page_texts(p: &PdfPage) -> Vec<&str> {
    p.ops
        .iter()
        .filter_map(|o| match o {
            PageOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Text of every `TH` label (header cells are letter-spaced labels, tagged with their text).
fn header_cells(pages: &[PdfPage]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter_map(|o| match o {
            PageOp::Leaf {
                tag: "TH",
                actual: Some(t),
            } => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// A text field of feature 1: `feat.1.text`, or a later part of it (`feat.1.text.2`, on any
/// page).
fn is_feat1_text(name: &str) -> bool {
    let base = base_id(name);
    base == "feat.1.text"
        || base
            .strip_prefix("feat.1.text.")
            .is_some_and(|n| n.parse::<u32>().is_ok())
}

fn feature_text_value(pages: &[PdfPage]) -> String {
    pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter_map(|o| match o {
            PageOp::Field(f) if is_feat1_text(&f.name) => match &f.kind {
                FieldKind::Text(t) => Some(t.value.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn feature_tables_are_drawn_as_ruled_tables_with_a_header_row() {
    for base in [data(&wizard_2014()), data(&wizard_2024())] {
        let mut d = base;
        d.features.insert(0, table_feature(4, 3, "Short value"));
        let (pages, _) = pages_of(&d, &LayoutOptions::default());
        let texts: Vec<&str> = pages.iter().flat_map(page_texts).collect();
        let heads = header_cells(&pages);
        for h in ["HEAD 0", "HEAD 1", "HEAD 2"] {
            assert!(heads.contains(&h.to_string()), "{:?}: {h} drawn", d.edition);
        }
        assert!(texts.contains(&"Option 3"));
        let text = feature_text_value(&pages);
        assert!(text.contains("Choose one option.") && text.contains("Then rest."));
        assert!(
            !text.contains("Option 0."),
            "rows leave the text field: {text}"
        );
        let groups: Vec<&str> = pages
            .iter()
            .flat_map(|p| &p.ops)
            .filter_map(|o| match o {
                PageOp::Group(g) => Some(*g),
                _ => None,
            })
            .collect();
        assert!(groups.contains(&"Table"));
    }
}

#[test]
fn long_feature_tables_continue_with_the_header_repeated() {
    let mut d = data(&wizard_2014());
    d.features
        .insert(0, table_feature(90, 3, "A somewhat longer cell value"));
    let (pages, _) = pages_of(&d, &LayoutOptions::default());
    let texts: Vec<&str> = pages.iter().flat_map(page_texts).collect();
    // Cells may wrap, so look at the drawn lines joined in drawing order.
    let joined = format!(" {} ", texts.join(" "));
    for r in 0..90 {
        let label = format!(" Option {r} ");
        assert_eq!(joined.matches(&label).count(), 1, "{label} printed once");
    }
    let heads = header_cells(&pages)
        .iter()
        .filter(|t| *t == "HEAD 1")
        .count();
    assert!(heads >= 2, "header repeated on the continuation ({heads})");
    assert!(texts.contains(&"Synthetic table feature (continued)"));
}

#[test]
fn a_table_without_room_stays_as_text() {
    let mut d = data(&wizard_2014());
    let word = "Incomprehensibilities".repeat(2);
    d.features.insert(0, table_feature(3, 6, &word));
    let (pages, _) = pages_of(&d, &LayoutOptions::default());
    let text = feature_text_value(&pages);
    assert!(
        text.contains("Option 0. Head 1:"),
        "row sentences kept: {text}"
    );
}

/// Top of the feature text field whose value contains `needle`, and its page.
fn text_field_at(pages: &[PdfPage], needle: &str) -> Option<(usize, f32, String)> {
    pages.iter().enumerate().find_map(|(p, page)| {
        page.ops.iter().find_map(|o| match o {
            PageOp::Field(f) if is_feat1_text(&f.name) => match &f.kind {
                FieldKind::Text(t) if t.value.contains(needle) => Some((p, f.y, f.name.clone())),
                _ => None,
            },
            _ => None,
        })
    })
}

#[test]
fn prose_after_a_feature_table_prints_below_it() {
    for base in [data(&wizard_2014()), data(&wizard_2024())] {
        let mut d = base;
        d.features.insert(0, table_feature(4, 3, "Short value"));
        let (pages, _) = pages_of(&d, &LayoutOptions::default());
        let (p0, y0, first) = text_field_at(&pages, "Choose one option.").expect("prose before");
        let (p1, y1, after) = text_field_at(&pages, "Then rest.").expect("prose after");
        assert_ne!(
            first, after,
            "{:?}: the later prose has a field of its own",
            d.edition
        );
        let last_row = pages[p1].ops.iter().find_map(|o| match o {
            PageOp::Text { text, y, .. } if text == "Option 3" => Some(*y),
            _ => None,
        });
        let row_y = last_row.expect("the last row is on the page of the later prose");
        assert!(p1 > p0 || y1 > y0, "{:?}: order kept", d.edition);
        assert!(
            y1 > row_y,
            "{:?}: prose {y1} below the last row {row_y}",
            d.edition
        );
    }
}

#[test]
fn a_header_too_long_for_one_line_wraps_and_the_table_is_drawn() {
    let mut d = data(&wizard_2014());
    let head = "Extraordinarily long heading label";
    d.features
        .insert(0, table_feature_with(3, 3, "Short value", head));
    let (pages, _) = pages_of(&d, &LayoutOptions::default());
    let text = feature_text_value(&pages);
    assert!(!text.contains("Option 0. "), "rows leave the text: {text}");
    let heads = header_cells(&pages);
    let label = format!("{} 1", head.to_uppercase());
    assert!(heads.contains(&label), "{label} in {heads:?}");
    let texts: Vec<&str> = pages.iter().flat_map(page_texts).collect();
    assert!(texts.contains(&"Option 2"));
}

#[test]
fn notes_with_text_still_get_their_page() {
    let o = LayoutOptions::default();
    let g = Grid::new(o.paper);
    for lines in [40, 60, 80] {
        let mut d = with_story(data(&wizard_2014()));
        d.story.treasure = numbered_lines(lines);
        let pages = build_with_limits(&d, &o, &g, 2, None).0.main;
        let names = field_names(&pages);
        assert!(
            names.iter().any(|n| base_id(n) == "story.notes"),
            "{lines}: notes with text are printed"
        );
    }
}
