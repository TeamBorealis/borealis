//! Spellcasting page tests and the visual sample writer.

use super::super::grid::Grid;
use super::super::{chrome, LayoutOptions};
use super::{build, build_with_limits, Built};
use crate::fonts::text_width;
use crate::model_fixtures::{
    cleric_2014, cleric_wizard_2014, fighter_wizard_2014, wizard_2014, wizard_2024,
};
use crate::model_types::{SheetData, SpellFacts, SpellFactsMap, SpellRow};
use crate::pdf::{write_document, FormMode, PageOp, PdfDocument};
use crate::pdf_draw::{Paint, Rgb};
use crate::pdf_form::{FieldKind, FormField};
use crate::Paper;
use std::collections::HashMap;

type Pair = (
    borealis_character::Character,
    borealis_engine::CharacterSnapshot,
);

fn data(pair: &Pair) -> SheetData {
    crate::sheet_data(&pair.0, &pair.1, &SpellFactsMap::new())
}

fn fixtures() -> Vec<(&'static str, SheetData)> {
    vec![
        ("wizard 2014", data(&wizard_2014())),
        ("wizard 2024", data(&wizard_2024())),
        ("cleric 2014", data(&cleric_2014())),
        ("fighter/wizard", data(&fighter_wizard_2014())),
        ("cleric/wizard", data(&cleric_wizard_2014())),
    ]
}

fn opts(paper: Paper) -> LayoutOptions {
    LayoutOptions {
        paper,
        ..LayoutOptions::default()
    }
}

fn run(d: &SheetData, o: &LayoutOptions, include: bool) -> Vec<Built> {
    build_with_limits(d, o, include, &Grid::new(o.paper), 3)
}

/// Spell fields only; the chrome header repeats page 1's `char.*` fields (the writer renames them).
fn fields(pages: &[Built]) -> Vec<(usize, &FormField)> {
    pages
        .iter()
        .enumerate()
        .flat_map(|(i, (p, _))| {
            p.ops.iter().filter_map(move |op| match op {
                PageOp::Field(f) if f.name.starts_with("spell.") => Some((i, f)),
                _ => None,
            })
        })
        .collect()
}

fn base_id(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

/// Spell ids the table is expected to print (pooled casters have no per-class slot trackers).
fn expected(d: &SheetData) -> Vec<String> {
    let pooled_classes: Vec<usize> = d
        .spellcasting
        .casters
        .iter()
        .enumerate()
        .filter(|(_, c)| c.slots_pooled)
        .map(|(i, _)| i + 1)
        .collect();
    d.fields()
        .into_iter()
        .map(|f| f.id)
        .filter(|id| id.starts_with("spell."))
        .filter(|id| {
            !pooled_classes
                .iter()
                .any(|n| id.starts_with(&format!("spell.{n}.slot.")))
        })
        .collect()
}

fn synthetic(rows: usize) -> SheetData {
    let mut d = data(&wizard_2014());
    let facts = SpellFacts {
        casting_time: Some("1 action".into()),
        range: Some("120 feet".into()),
        components: Some("V, S, M (a bit of bat fur)".into()),
        concentration: true,
        ritual: true,
        effect: Some("8d6 fire, DEX save, half on save, 20-foot-radius sphere of flame".into()),
        ..SpellFacts::default()
    };
    let caster = &mut d.spellcasting.casters[0];
    caster.spells = (0..rows)
        .map(|i| SpellRow {
            id: format!("ID_SYN_{i}"),
            name: format!("Synthetic Spell Number {i:03}"),
            level: 1 + (i * 9 / rows.max(1)) as u32,
            prepared: i % 3 != 0,
            always_prepared: i % 17 == 0,
            known: true,
            source: None,
            damage: None,
            facts: Some(facts.clone()),
        })
        .collect();
    d
}

#[test]
fn every_spell_field_appears_exactly_once() {
    for (name, d) in fixtures() {
        for paper in [Paper::Letter, Paper::A4] {
            let pages = run(&d, &opts(paper), true);
            let mut seen: HashMap<String, usize> = HashMap::new();
            for (_, f) in fields(&pages) {
                *seen.entry(base_id(&f.name).to_string()).or_default() += 1;
            }
            for id in expected(&d) {
                assert_eq!(seen.get(&id).copied(), Some(1), "{name}: {id}");
            }
            let want = expected(&d);
            for id in seen.keys() {
                assert!(want.contains(id), "{name}: unexpected field {id}");
            }
        }
    }
}

#[test]
fn unprepared_spells_are_dropped_only_when_asked() {
    let d = data(&cleric_2014());
    let all = fields(&run(&d, &opts(Paper::Letter), true)).len();
    let some = fields(&run(&d, &opts(Paper::Letter), false)).len();
    let c = &d.spellcasting.casters[0];
    assert!(c.prepares && c.listed(false).len() < c.spells.len());
    assert!(some < all);
}

#[test]
fn continuation_keeps_every_row_whole() {
    let d = synthetic(120);
    for paper in [Paper::Letter, Paper::A4] {
        let o = opts(paper);
        let g = Grid::new(paper);
        let pages = run(&d, &o, true);
        assert!(pages.len() > 2, "{} pages", pages.len());
        let c = &d.spellcasting.casters[0];
        let rows = c.cantrips.len() + c.spells.len();
        let mut by_row: HashMap<usize, Vec<(usize, f32)>> = HashMap::new();
        for (page, f) in fields(&pages) {
            let id = base_id(&f.name);
            if page > 0 {
                assert!(f.name.ends_with(&format!("@{}", page + 1)), "{}", f.name);
            } else {
                assert!(!f.name.contains('@'), "{}", f.name);
            }
            assert!(f.y + f.h <= g.body_bottom, "{id} below the body");
            if let Some(rest) = id.strip_prefix("spell.1.list.") {
                let n: usize = rest.split('.').next().unwrap().parse().unwrap();
                by_row.entry(n).or_default().push((page, f.y));
            }
        }
        assert_eq!(by_row.len(), rows, "every row printed");
        for (n, spots) in &by_row {
            let first = spots[0];
            assert!(
                spots
                    .iter()
                    .all(|s| s.0 == first.0 && (s.1 - first.1).abs() < super::spells_table::ROW_H),
                "row {n} is split"
            );
        }
        let texts: Vec<&str> = pages
            .iter()
            .flat_map(|(p, _)| p.ops.iter())
            .filter_map(|op| match op {
                PageOp::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"Spellcasting (continued)"));
        assert!(texts.contains(&"(cont.)"), "repeated level header");
        assert!(texts.iter().any(|t| t.starts_with("Continued on page 4")));
    }
}

#[test]
fn nothing_exceeds_its_box() {
    let mut sets: Vec<(&str, SheetData)> = fixtures();
    sets.push(("synthetic", synthetic(120)));
    for (name, d) in sets {
        for (paper, compact) in [(Paper::Letter, false), (Paper::A4, true)] {
            let o = LayoutOptions {
                paper,
                compact,
                ..LayoutOptions::default()
            };
            for (page, limits) in run(&d, &o, true) {
                for (i, limit) in &limits {
                    if let PageOp::Text {
                        style, size, text, ..
                    } = &page.ops[*i]
                    {
                        let w = text_width(*style, text, *size);
                        assert!(w <= limit + 0.05, "{name}: {text:?} is {w}, limit {limit}");
                    }
                }
                for op in &page.ops {
                    let PageOp::Field(f) = op else { continue };
                    if let FieldKind::Text(t) = &f.kind {
                        let w = text_width(t.style, &t.value, t.size);
                        assert!(
                            w <= f.w - 2.0 * t.pad + 0.05 || t.size <= 4.81,
                            "{name}: {} {:?}",
                            f.name,
                            t.value
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
    let mut sets: Vec<(&str, SheetData)> = fixtures();
    sets.push(("synthetic", synthetic(120)));
    for (name, d) in sets {
        let o = LayoutOptions {
            ink: true,
            ..LayoutOptions::default()
        };
        for page in build(&d, &o, true, &Grid::new(o.paper), 3) {
            for op in &page.ops {
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
                assert!(ok, "{name}: non-grey colour in ink mode: {op:?}");
            }
        }
    }
}

#[test]
fn non_caster_has_no_spell_pages() {
    let mut d = data(&wizard_2014());
    d.spellcasting = crate::model_types::SpellcastingData::default();
    let o = opts(Paper::Letter);
    assert!(build(&d, &o, true, &Grid::new(o.paper), 3).is_empty());
}

fn warlock() -> SheetData {
    let mut d = synthetic(8);
    let caster = &mut d.spellcasting.casters[0];
    caster.slots = [0; 9];
    caster.pact = Some(crate::model_types::PactSlots { count: 2, level: 3 });
    d
}

#[test]
fn pact_slots_have_their_own_fields() {
    let d = warlock();
    let pages = run(&d, &opts(Paper::Letter), true);
    let names: Vec<&str> = fields(&pages)
        .iter()
        .map(|(_, f)| base_id(&f.name))
        .collect();
    for id in [
        "spell.1.pact.total",
        "spell.1.pact.level",
        "spell.1.pact.used.1",
        "spell.1.pact.used.2",
    ] {
        assert_eq!(names.iter().filter(|n| **n == id).count(), 1, "{id}");
    }
}

#[test]
fn pooled_table_replaces_class_trackers() {
    let d = data(&cleric_wizard_2014());
    assert!(d.spellcasting.pooled.is_some());
    let pages = run(&d, &opts(Paper::Letter), true);
    let names: Vec<&str> = fields(&pages)
        .iter()
        .map(|(_, f)| base_id(&f.name))
        .collect();
    assert!(names.contains(&"spell.pool.level"));
    assert!(!names.iter().any(|n| n.starts_with("spell.1.slot.")));
}

fn write_doc(path: &str, d: &SheetData, o: &LayoutOptions, include: bool) {
    let g = Grid::new(o.paper);
    let mut pages: Vec<_> = build(d, o, include, &g, 1);
    chrome::footers(&mut pages, &g, d, o.ink);
    let doc = PdfDocument {
        page_w: g.w,
        page_h: g.h,
        pages,
        images: Vec::new(),
        title: "E2b sample".to_string(),
        lang: Some("en".to_string()),
        mode: FormMode::Fillable,
        structure: None,
    };
    std::fs::write(path, write_document(&doc)).unwrap();
}

/// Writes samples when `BOREALIS_E2B_SAMPLE` names an output directory (visual check).
#[test]
fn write_samples() {
    let Ok(dir) = std::env::var("BOREALIS_E2B_SAMPLE") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let sets = [
        ("2014", data(&wizard_2014())),
        ("2024", data(&wizard_2024())),
        ("2014multi", data(&cleric_wizard_2014())),
        ("2014c", data(&cleric_2014())),
        ("long", synthetic(120)),
        ("pact", warlock()),
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
            write_doc(&format!("{dir}/{name}-{tag}.pdf"), d, &o, true);
        }
    }
}

#[test]
fn level_slots_put_pact_slots_at_their_level() {
    let d = warlock();
    let c = &d.spellcasting.casters[0];
    assert_eq!(c.level_slots(3), 2);
    assert_eq!(
        (c.level_slots(1), c.level_slots(0), c.level_slots(10)),
        (0, 0, 0)
    );
}
