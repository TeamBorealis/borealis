//! Page 1 tests and the visual sample writer.

use super::grid::Grid;
use super::page_core::{self, Page1};
use super::{build_document, LayoutOptions};
use crate::fonts::text_width;
use crate::model_fixtures::{cleric_2014, wizard_2014, wizard_2024};
use crate::model_types::{SheetData, SpellFactsMap};
use crate::pdf::{write_document, PageOp};
use crate::pdf_draw::{Paint, Rgb};
use crate::pdf_form::FieldKind;
use crate::Paper;
use std::collections::HashMap;

fn data(
    pair: &(
        borealis_character::Character,
        borealis_engine::CharacterSnapshot,
    ),
) -> SheetData {
    crate::sheet_data(&pair.0, &pair.1, &SpellFactsMap::new())
}

fn page1(d: &SheetData, o: &LayoutOptions) -> Page1 {
    let g = Grid::new(o.paper);
    let refs = page_core::PageRefs {
        continuation: 2,
        band: super::BandRefs {
            details: 2,
            inventory: Some(4),
        },
    };
    page_core::build(d, o, &g, refs, page_core::band(d, o, &g).as_ref())
}

fn fixtures() -> Vec<SheetData> {
    vec![
        data(&wizard_2014()),
        data(&wizard_2024()),
        data(&cleric_2014()),
    ]
}

/// Whether a field id is printed on page 1.
fn on_page_1(id: &str, d: &SheetData) -> bool {
    let n_of = |prefix: &str| -> Option<usize> {
        id.strip_prefix(prefix)?.split('.').next()?.parse().ok()
    };
    if let Some(n) = n_of("atk.") {
        return n <= super::lists::ATTACK_CAPACITY;
    }
    if let Some(n) = n_of("use.") {
        return n <= super::lists::USE_CAPACITY;
    }
    if id.starts_with("summary.equip.") {
        // The 2014 summary prints as many lines as the band has room for.
        return false;
    }
    if id.starts_with("summary.coin.") {
        return true;
    }
    let _ = d;
    const PREFIXES: [&str; 17] = [
        "abil.", "save.", "skill.", "passive.", "hd.", "death.", "cond.", "exh.", "prof.",
        "armor.", "mastery.", "insp", "ac", "hp.", "init", "speed", "senses",
    ];
    const EXACT: [&str; 14] = [
        "char.name",
        "char.player",
        "char.class",
        "char.subclass",
        "char.level",
        "char.background",
        "char.alignment",
        "char.xp",
        "char.xp_next",
        "char.size",
        "char.race",
        "char.subrace",
        "char.species",
        "char.lineage",
    ];
    PREFIXES.iter().any(|p| id.starts_with(p))
        || EXACT.contains(&id)
        || id == "defenses"
        || id == "char.origin_feat"
}

fn field_names(p: &Page1) -> Vec<String> {
    p.page
        .ops
        .iter()
        .filter_map(|o| {
            if let PageOp::Field(f) = o {
                Some(f.name.clone())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn every_page_1_id_appears_exactly_once() {
    for d in fixtures() {
        let p = page1(&d, &LayoutOptions::default());
        let names = field_names(&p);
        let mut count: HashMap<&str, usize> = HashMap::new();
        for n in &names {
            *count.entry(n.as_str()).or_default() += 1;
        }
        let known: Vec<String> = d.fields().into_iter().map(|f| f.id).collect();
        for id in &known {
            if on_page_1(id, &d) {
                assert_eq!(
                    count.get(id.as_str()).copied().unwrap_or(0),
                    1,
                    "{id} ({:?})",
                    d.edition
                );
            }
        }
        for n in &names {
            assert!(known.contains(n), "invented id {n}");
            assert_eq!(count[n.as_str()], 1, "duplicate {n}");
        }
    }
}

#[test]
fn no_text_exceeds_its_box() {
    for d in fixtures() {
        for paper in [Paper::Letter, Paper::A4] {
            for compact in [false, true] {
                let o = LayoutOptions {
                    paper,
                    compact,
                    ..LayoutOptions::default()
                };
                let p = page1(&d, &o);
                for (i, limit) in &p.limits {
                    if let PageOp::Text {
                        style, size, text, ..
                    } = &p.page.ops[*i]
                    {
                        let w = text_width(*style, text, *size);
                        assert!(w <= limit + 0.05, "{text:?} is {w} wide, limit {limit}");
                    }
                }
                for op in &p.page.ops {
                    if let PageOp::Field(f) = op {
                        if let FieldKind::Text(t) = &f.kind {
                            if !t.multiline {
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
        let doc = build_document(&d, &o);
        for op in &doc.pages[0].ops {
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

#[test]
fn document_serialises_and_counts_pages() {
    let d = data(&wizard_2024());
    for fillable in [true, false] {
        let doc = build_document(
            &d,
            &LayoutOptions {
                fillable,
                ..LayoutOptions::default()
            },
        );
        assert!(write_document(&doc).starts_with(b"%PDF"));
    }
}

/// Writes page 1 samples when `BOREALIS_E1_SAMPLE` names an output directory (visual check).
#[test]
fn write_samples() {
    let Ok(dir) = std::env::var("BOREALIS_E1_SAMPLE") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let sets = [
        ("2014", data(&wizard_2014())),
        ("2024", data(&wizard_2024())),
        ("2014c", data(&cleric_2014())),
        (
            "2014-band",
            crate::model_fixtures::with_sample_extras(data(&cleric_2014())),
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
            std::fs::write(
                format!("{dir}/{name}-{tag}.pdf"),
                write_document(&build_document(d, &o)),
            )
            .unwrap();
        }
    }
}

#[test]
fn ruled_text_keeps_descenders_clear_of_the_rule() {
    use super::grid::ruled_baseline;
    for (pitch, size) in [(12.0, 8.0), (11.0, 7.5), (13.0, 9.0)] {
        let gap = pitch - ruled_baseline(pitch, size);
        assert!(gap >= size * 0.27 + 1.0, "{gap} for {size} on {pitch}");
        // The cap height still fits above the first baseline.
        assert!(ruled_baseline(pitch, size) > size * 0.75);
    }
}
