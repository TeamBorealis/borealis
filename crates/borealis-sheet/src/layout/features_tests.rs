//! Tests of the features, gear and story pages, and the visual sample writer.

use super::build_with_limits;
use super::features_text::wrap_field;
use crate::fonts::text_width;
use crate::layout::grid::Grid;
use crate::layout::page_story;
use crate::layout::{chrome, LayoutOptions};
use crate::model_fixtures::{cleric_2014, wizard_2014, wizard_2024};
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
    let (f, mut limits) = build_with_limits(d, o, &g, 2);
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
    let story_page = d.edition == Edition::Dnd2024 && d.story.has_content();
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
