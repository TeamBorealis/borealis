//! The 2014 page-1 band: personality and equipment with coins (M8), and the hand-over of
//! personality text that does not fit to the Details page.

use super::{share_lines, Band, BandRefs, PERSONALITY};
use crate::layout::grid::Grid;
use crate::layout::page_core::{self, PageRefs};
use crate::layout::page_features;
use crate::layout::LayoutOptions;
use crate::model_fixtures::{cleric_2014, wizard_2014, wizard_2024};
use crate::model_types::{GearItem, SheetData, SpellFactsMap};
use crate::pdf::{PageOp, PdfPage};
use crate::pdf_form::FieldKind;
use crate::Paper;

type Pair = (
    borealis_character::Character,
    borealis_engine::CharacterSnapshot,
);

fn data(p: &Pair) -> SheetData {
    crate::sheet_data(&p.0, &p.1, &SpellFactsMap::new())
}

const REFS: PageRefs = PageRefs {
    continuation: 2,
    band: BandRefs {
        details: 2,
        inventory: Some(5),
    },
};

fn page_one(d: &SheetData, o: &LayoutOptions) -> (PdfPage, Option<Band>) {
    let g = Grid::new(o.paper);
    let band = page_core::band(d, o, &g);
    (page_core::build(d, o, &g, REFS, band.as_ref()).page, band)
}

fn texts(p: &PdfPage) -> Vec<&str> {
    p.ops
        .iter()
        .filter_map(|o| match o {
            PageOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// `(name, value)` of every text field.
fn fields(pages: &[PdfPage]) -> Vec<(String, String)> {
    pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter_map(|o| match o {
            PageOp::Field(f) => match &f.kind {
                FieldKind::Text(t) => Some((f.name.clone(), t.value.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn words(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

fn item(n: usize) -> GearItem {
    GearItem {
        name: format!("Item {n}"),
        quantity: 1,
        weight: 1.0,
        equipped: String::new(),
        attuned: false,
        requires_attunement: false,
    }
}

#[test]
fn lines_go_to_the_longest_text_first_then_to_the_shortest_block() {
    assert_eq!(share_lines(&[0, 0, 0, 0], 7), [2, 2, 2, 1]);
    assert_eq!(share_lines(&[5, 1, 0, 2], 7), [4, 1, 1, 1]);
    assert_eq!(share_lines(&[2, 0, 0, 3], 8), [2, 2, 1, 3]);
    assert_eq!(share_lines(&[9, 9, 9, 9], 6), [2, 2, 1, 1]);
    assert_eq!(share_lines(&[1, 1, 1, 1], 4), [1, 1, 1, 1]);
}

#[test]
fn page_1_of_2014_carries_personality_and_equipment_with_coins() {
    for pair in [wizard_2014(), cleric_2014()] {
        let mut d = data(&pair);
        d.story.ideals = "Knowledge.".into();
        d.gear.coins = [1, 2, 3, 1234, 5];
        for paper in [Paper::Letter, Paper::A4] {
            for compact in [false, true] {
                let o = LayoutOptions {
                    paper,
                    compact,
                    ..LayoutOptions::default()
                };
                let (p, band) = page_one(&d, &o);
                assert!(band.is_some(), "{paper:?} compact {compact}");
                let t = texts(&p);
                for heading in ["Personality", "Equipment"] {
                    assert!(t.contains(&heading), "{heading} on page 1");
                }
                assert!(!t.contains(&"Notes"), "the band replaces the Notes filler");
                let f = fields(std::slice::from_ref(&p));
                assert!(
                    f.iter()
                        .any(|(n, v)| n == "summary.coin.gp" && v == "1,234"),
                    "coins on page 1 are fields of their own"
                );
                for c in crate::model_ids::SUMMARY_COINS {
                    assert_eq!(f.iter().filter(|(n, _)| n == c).count(), 1, "{c}");
                }
                for (id, _) in PERSONALITY {
                    assert_eq!(f.iter().filter(|(n, _)| n == id).count(), 1, "{id}");
                }
                assert!(f
                    .iter()
                    .any(|(n, v)| n == "story.ideals" && v == "Knowledge."));
                assert!(
                    !f.iter().any(|(n, _)| n.starts_with("coin.")),
                    "coin fields stay on the inventory page"
                );
            }
        }
        let (p, _) = page_one(&d, &LayoutOptions::default());
        let f = fields(std::slice::from_ref(&p));
        for (i, it) in d.gear.items.iter().enumerate() {
            let name = crate::model_ids::summary_equip(i + 1, "name");
            let qty = crate::model_ids::summary_equip(i + 1, "qty");
            assert!(f.contains(&(name, it.name.clone())), "{}", it.name);
            assert!(f.contains(&(qty, it.quantity.to_string())), "{}", it.name);
        }
        assert!(
            !f.iter().any(|(n, _)| n.starts_with("equip.")),
            "inventory fields stay on the inventory page"
        );
        // Lines past the last item are blank write-in fields (widgets when fillable).
        let blank = crate::model_ids::summary_equip(d.gear.items.len() + 1, "name");
        assert!(f.contains(&(blank, String::new())), "blank summary line");
    }
}

#[test]
fn page_1_of_2024_has_no_band() {
    let d = data(&wizard_2024());
    let (p, band) = page_one(&d, &LayoutOptions::default());
    assert!(band.is_none());
    assert!(texts(&p).contains(&"Notes"));
}

#[test]
fn a_long_equipment_list_points_at_the_inventory_page() {
    let mut d = data(&wizard_2014());
    d.gear.items = (1..=40).map(item).collect();
    let (p, _) = page_one(&d, &LayoutOptions::default());
    let t = texts(&p);
    let f = fields(std::slice::from_ref(&p));
    let shown = (1..=40)
        .filter(|n| f.iter().any(|(_, v)| v == &format!("Item {n}")))
        .count();
    assert!(f.iter().any(
        |(n, v)| n == &crate::model_ids::summary_equip(shown, "name")
            && v == &format!("Item {shown}")
    ));
    assert!((2..15).contains(&shown), "{shown} lines");
    let note = format!("+{} more on page 5", 40 - shown);
    assert!(t.contains(&note.as_str()), "{note}");
    // A tagged table: a header row plus one TR per printed line, after the Equipment heading.
    let at = p
        .ops
        .iter()
        .position(|o| matches!(o, PageOp::Text { text, .. } if text == "Equipment"))
        .expect("heading");
    let rows = p.ops[at..]
        .iter()
        .filter(|o| matches!(o, PageOp::Group("TR")))
        .count();
    let tables = p.ops[at..]
        .iter()
        .filter(|o| matches!(o, PageOp::Group("Table")))
        .count();
    assert_eq!((tables, rows), (1, shown + 1));
}

#[test]
fn personality_that_does_not_fit_goes_on_on_the_details_page() {
    let mut d = data(&wizard_2014());
    let long = (1..=60)
        .map(|n| format!("word{n}"))
        .collect::<Vec<_>>()
        .join(" ");
    d.story.traits = long.clone();
    d.story.flaws = "Short.".into();
    let o = LayoutOptions::default();
    let g = Grid::new(o.paper);
    let band = page_core::band(&d, &o, &g).expect("band");
    let (p1, _) = page_one(&d, &o);
    assert!(texts(&p1).contains(&"continued on page 2"));
    let details = page_features::build(&d, &o, &g, 2, Some(&band)).main;
    let f1 = fields(std::slice::from_ref(&p1));
    let fd = fields(&details);
    let head = &f1.iter().find(|(n, _)| n == "story.traits").unwrap().1;
    let tail = &fd.iter().find(|(n, _)| n == "story.traits@2").unwrap().1;
    let mut all = words(head);
    all.extend(words(tail));
    assert_eq!(all, words(&long), "nothing lost or doubled");
    assert!(texts(&details[0]).contains(&"Personality traits (continued)"));
    // Blocks that fit are printed once, on page 1 only.
    assert!(!fd.iter().any(|(n, _)| n.starts_with("story.flaws")));
    assert!(fd.iter().any(|(n, _)| n == "story.backstory"));
}

#[test]
fn without_page_1_the_details_page_keeps_the_personality_blocks() {
    let mut d = data(&wizard_2014());
    d.story.bonds = "My tower.".into();
    let o = LayoutOptions::default();
    let g = Grid::new(o.paper);
    let details = page_features::build(&d, &o, &g, 1, None).main;
    let fd = fields(&details);
    for (id, _) in PERSONALITY {
        assert_eq!(fd.iter().filter(|(n, _)| n == id).count(), 1, "{id}");
    }
    assert!(fd
        .iter()
        .any(|(n, v)| n == "story.bonds" && v == "My tower."));
}
