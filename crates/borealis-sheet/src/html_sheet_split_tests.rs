//! B-146: the HTML preview splits its pages as the PDF does. Page 1's 2014 band (personality
//! lines and equipment lines), its features summary, the attacks and limited uses past page 1
//! and the spellcasting pages all come from the PDF's own plan.

use super::parts::esc;
use super::{layout_options, render_sheet, HtmlOptions};
use crate::layout::{self, Band, Grid};
use crate::model_fixtures::{cleric_2014, cleric_wizard_2014, wizard_2014, wizard_2024};
use crate::model_ids as id;
use crate::model_types::{GearItem, SheetData, SpellFactsMap};
use crate::Paper;

type Pair = (
    borealis_character::Character,
    borealis_engine::CharacterSnapshot,
);

fn data(p: &Pair) -> SheetData {
    let mut d = crate::sheet_data(&p.0, &p.1, &SpellFactsMap::new());
    d.story.backstory = "Raised by the lighthouse keeper.".into();
    d
}

/// The page sections of a document, in order.
fn pages(html: &str) -> Vec<&str> {
    html.split("<section class=\"page ").skip(1).collect()
}

fn kind(page: &str) -> &str {
    let at = page.find("data-page=\"").map_or(0, |i| i + 11);
    let rest = &page[at..];
    &rest[..rest.find('"').unwrap_or(0)]
}

fn page_of<'a>(html: &'a str, k: &str) -> &'a str {
    pages(html)
        .into_iter()
        .find(|p| kind(p) == k)
        .unwrap_or_else(|| panic!("no {k} page"))
}

/// 1-based number of the first page of kind `k`.
fn number_of(html: &str, k: &str) -> usize {
    pages(html).iter().position(|p| kind(p) == k).expect(k) + 1
}

fn has_field(page: &str, fid: &str) -> bool {
    page.contains(&format!("data-field=\"{fid}\""))
}

/// The text of the element carrying `fid`.
fn field_text(page: &str, fid: &str) -> Option<String> {
    let at = page.find(&format!("data-field=\"{fid}\""))?;
    let rest = &page[at..];
    let start = rest.find('>')? + 1;
    let end = rest[start..].find('<')? + start;
    Some(rest[start..end].to_string())
}

fn band(d: &SheetData, o: &HtmlOptions) -> Option<Band> {
    layout::page_one_band(d, &layout_options(o), &Grid::new(o.paper))
}

fn item(n: usize) -> GearItem {
    GearItem {
        name: format!("Trinket number {n}"),
        quantity: 1,
        weight: 1.0,
        equipped: String::new(),
        attuned: false,
        requires_attunement: false,
    }
}

const LONG: &str =
    "I keep a careful list of every promise I have made and every one I have broken, \
and I read it aloud whenever the night is quiet enough to hear myself think.";

#[test]
fn personality_lines_and_their_rest_follow_the_pdf_band() {
    for paper in [Paper::Letter, Paper::A4] {
        for compact in [false, true] {
            let mut d = data(&cleric_2014());
            d.story.traits = [LONG; 4].join(" ");
            d.story.ideals = "Knowledge.".into();
            d.story.bonds = [LONG; 2].join(" ");
            let o = HtmlOptions {
                paper,
                compact,
                ..HtmlOptions::default()
            };
            let b = band(&d, &o).expect("the cleric's page 1 has its band");
            assert!(b.traits.iter().any(|t| t.tail.is_some()), "{paper:?}");
            let html = render_sheet(&d, &o);
            let (p1, details) = (page_of(&html, "character"), page_of(&html, "details"));
            let details_no = number_of(&html, "details");
            for t in &b.traits {
                let at = format!("{paper:?} compact {compact} {}", t.id);
                assert_eq!(
                    field_text(p1, t.id),
                    Some(esc(&t.head_lines.join("\n"))),
                    "{at}"
                );
                assert!(t.head_lines.len() <= t.lines, "{at}: the PDF's lines");
                assert_eq!(
                    t.head_lines
                        .join(" ")
                        .split_whitespace()
                        .collect::<Vec<_>>(),
                    t.head.split_whitespace().collect::<Vec<_>>(),
                    "{at}"
                );
                let lines = format!("height:{:.2}pt", t.lines as f32 * b.pitch);
                assert!(p1.contains(&lines), "{at}: {lines}");
                assert!(!has_field(details, t.id), "{at}: only page 1 has the block");
                let rest = format!("{}@2", t.id);
                match &t.tail {
                    Some(tail) => {
                        assert_eq!(field_text(details, &rest), Some(esc(tail.trim())), "{at}");
                        assert!(
                            details.contains(&format!("{} (continued)", t.title)),
                            "{at}"
                        );
                    }
                    None => assert!(!has_field(details, &rest), "{at}"),
                }
            }
            assert!(p1.contains(&format!("continued on page {details_no}")));
        }
    }
}

#[test]
fn without_a_band_page_1_keeps_its_notes_and_details_the_personality_blocks() {
    use crate::model_fixtures::{long_vitals, with_sample_extras};
    // The widest vitals leave no room for the band on page 1 (the PDF keeps its Notes too).
    let d = long_vitals(with_sample_extras(data(&cleric_2014())));
    let o = HtmlOptions::default();
    assert!(band(&d, &o).is_none());
    let html = render_sheet(&d, &o);
    let p1 = page_of(&html, "character");
    assert!(p1.contains(">Notes<"));
    assert!(!has_field(p1, "story.traits") && !has_field(p1, "summary.coin.gp"));
    let details = page_of(&html, "details");
    for fid in ["story.traits", "story.ideals", "story.bonds", "story.flaws"] {
        assert!(has_field(details, fid), "{fid}");
        assert!(!has_field(details, &format!("{fid}@2")), "{fid}");
    }
}

#[test]
fn equipment_lines_stop_where_the_pdf_cuts_them() {
    for items in [2, 30] {
        let mut d = data(&cleric_2014());
        d.gear.items = (1..=items).map(item).collect();
        let o = HtmlOptions::default();
        let b = band(&d, &o).expect("band");
        let (shown, hidden) = b.equipment_cut(items);
        let html = render_sheet(&d, &o);
        let p1 = page_of(&html, "character");
        let printed = if hidden == 0 { b.rows } else { shown };
        for n in 1..=id::SUMMARY_ROWS {
            let want = usize::from(n <= printed);
            for part in ["qty", "name"] {
                let fid = id::summary_equip(n, part);
                assert_eq!(
                    p1.matches(&format!("data-field=\"{fid}\"")).count(),
                    want,
                    "{items} items: {fid}"
                );
            }
        }
        for n in 1..=shown {
            let name = field_text(p1, &id::summary_equip(n, "name"));
            assert_eq!(name, Some(format!("Trinket number {n}")));
        }
        let note = format!("+{hidden} more on page {}", number_of(&html, "inventory"));
        assert_eq!(p1.contains(&note), hidden > 0, "{items} items: {note}");
    }
}

#[test]
fn features_summary_lists_the_names_the_pdf_lists() {
    let mut d = data(&cleric_2014());
    let base = d.features[0].clone();
    d.features = (1..=40)
        .map(|n| crate::model_types::FeatureData {
            name: format!("Feature number {n}"),
            ..base.clone()
        })
        .collect();
    let o = HtmlOptions::default();
    let shown = layout::summary_feature_count(&d, &layout_options(&o), &Grid::new(o.paper));
    assert!((1..40).contains(&shown), "{shown}");
    let html = render_sheet(&d, &o);
    let p1 = page_of(&html, "character");
    for n in 1..=40 {
        let name = format!(">Feature number {n}<");
        assert_eq!(p1.contains(&name), n <= shown, "{name}");
    }
    let note = format!(
        "+{} more on page {}",
        40 - shown,
        number_of(&html, "details")
    );
    assert!(p1.contains(&note), "{note}");
}

#[test]
fn attacks_and_uses_past_page_1_go_on_the_overflow_page() {
    for pair in [cleric_2014(), wizard_2024()] {
        let mut d = data(&pair);
        let attack = d.combat.attacks[0].clone();
        d.combat.attacks = vec![attack; layout::ATTACK_CAPACITY + 2];
        let base = d.limited_uses.first().cloned();
        if let Some(u) = base {
            d.limited_uses = vec![u; layout::USE_CAPACITY + 3];
        }
        let html = render_sheet(&d, &HtmlOptions::default());
        assert_eq!(number_of(&html, "overflow"), 2, "the page after page 1");
        let (p1, over) = (page_of(&html, "character"), page_of(&html, "overflow"));
        for n in 1..=d.combat.attacks.len() {
            let fid = id::atk(n, "name");
            let on_one = n <= layout::ATTACK_CAPACITY;
            assert_eq!(
                (has_field(p1, &fid), has_field(over, &fid)),
                (on_one, !on_one),
                "{fid}"
            );
        }
        for n in 1..=d.limited_uses.len() {
            let fid = id::use_name(n);
            let on_one = n <= layout::USE_CAPACITY;
            assert_eq!(
                (has_field(p1, &fid), has_field(over, &fid)),
                (on_one, !on_one),
                "{fid}"
            );
        }
        assert!(p1.contains("+2 more on page 2"));
    }
    let html = render_sheet(&data(&cleric_2014()), &HtmlOptions::default());
    assert!(
        !html.contains("data-page=\"overflow\""),
        "only when page 1 overflows"
    );
}

#[test]
fn spellcasting_pages_hold_the_rows_of_the_pdf_pages() {
    let mut split_seen = false;
    for pair in [
        cleric_2014(),
        wizard_2014(),
        cleric_wizard_2014(),
        wizard_2024(),
    ] {
        let d = data(&pair);
        for paper in [Paper::Letter, Paper::A4] {
            let o = HtmlOptions {
                paper,
                ..HtmlOptions::default()
            };
            let lo = layout_options(&o);
            let g = Grid::new(paper);
            let plan = layout::spell_plan(&d, &lo, true, &g);
            assert_eq!(plan.len(), layout::build_spells(&d, &lo, true, &g, 1).len());
            split_seen |= plan.len() > 1;
            let html = render_sheet(&d, &o);
            let spell_pages: Vec<&str> = pages(&html)
                .into_iter()
                .filter(|p| kind(p) == "spellcasting")
                .collect();
            assert_eq!(spell_pages.len(), plan.len());
            let first = number_of(&html, "spellcasting");
            for (i, (page, pp)) in spell_pages.iter().zip(&plan).enumerate() {
                for b in &pp.blocks {
                    for &idx in &b.rows {
                        let fid = id::spell_row(b.caster, idx, "name");
                        assert!(has_field(page, &fid), "page {i}: {fid}");
                    }
                    let head = id::spell_head(b.caster, "dc");
                    assert_eq!(has_field(page, &head), b.head, "page {i}: {head}");
                }
                let cont = format!("Continued on page {}", first + i + 1);
                assert_eq!(page.contains(&cont), i + 1 < plan.len(), "page {i}");
            }
        }
    }
    assert!(split_seen, "a sample runs over two spellcasting pages");
}

fn is_flow(page: &str) -> bool {
    page[..page.find('>').unwrap_or(0)].contains(" flow\"")
}

#[test]
fn pages_keep_the_paper_height_unless_the_pdf_continues_them() {
    let d = crate::model_fixtures::with_sample_extras(data(&cleric_2014()));
    let html = render_sheet(&d, &HtmlOptions::default());
    for p in pages(&html) {
        assert!(!is_flow(p), "{} fits one PDF page", kind(p));
    }
    // A Details page the PDF continues on a second page grows instead of cutting text off.
    let mut long = d.clone();
    let base = long.features[1].clone();
    long.features = (1..=30)
        .map(|n| crate::model_types::FeatureData {
            name: format!("Feature number {n}"),
            text: [LONG; 3].join(" "),
            ..base.clone()
        })
        .collect();
    let o = HtmlOptions::default();
    let lo = layout_options(&o);
    let g = Grid::new(o.paper);
    let b = band(&long, &o);
    assert!(
        layout::build_features(&long, &lo, &g, 1, b.as_ref())
            .main
            .len()
            > 1
    );
    let html = render_sheet(&long, &o);
    for p in pages(&html) {
        assert_eq!(is_flow(p), kind(p) == "details", "{}", kind(p));
    }
}

#[test]
fn pdf_features_summary_points_at_the_details_page() {
    use crate::layout::{BandRefs, LayoutOptions, PageRefs};
    let mut d = data(&cleric_2014());
    let base = d.features[0].clone();
    d.features = (1..=40)
        .map(|n| crate::model_types::FeatureData {
            name: format!("Feature number {n}"),
            ..base.clone()
        })
        .collect();
    let o = LayoutOptions::default();
    let g = Grid::new(o.paper);
    let refs = PageRefs {
        continuation: 2,
        band: BandRefs {
            details: 3,
            inventory: None,
        },
    };
    let b = layout::page_one_band(&d, &o, &g);
    let page = layout::build_page_one(&d, &o, &g, refs, b.as_ref()).page;
    let shown = layout::summary_feature_count(&d, &o, &g);
    let note = format!("+{} more on page 3", 40 - shown);
    let found = page
        .ops
        .iter()
        .any(|op| matches!(op, crate::pdf::PageOp::Text { text, .. } if *text == note));
    assert!(found, "{note}");
}
