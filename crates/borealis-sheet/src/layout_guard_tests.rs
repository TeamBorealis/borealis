//! Fit and overlap guard: for every page of every fixture (both editions, Letter and A4, colour
//! and ink, cards and legacy spells, plus long-text stress characters), every text run and every
//! field value must sit inside its page (and a field value inside its box with the field padding),
//! no two text boxes on a page may intersect, and no rule (a drawn line or the header divider
//! ornament) may cross a text box. Measured with the writer's own font metrics.
//! Fillable and flattened sheets share these page ops (only the writer differs), and PDFKit's
//! drawing of the written file is covered by `tests/pdfkit_parity.rs`.

use crate::art::{self, ArtId, ArtPiece, ArtSet, Op};
use crate::fonts::{text_width, Style};
use crate::model_fixtures::{
    cleric_2014, cleric_wizard_2014, fighter_wizard_2014, wizard_2014, wizard_2024,
};
use crate::model_types::{FeatureData, SheetData, SpellFactsMap};
use crate::pdf::{PageOp, BASELINE};
use crate::pdf_form::{value_lines, FieldKind};
use crate::sheet_render::build_pages;
use crate::{CardKinds, Paper, SheetContent, SheetEdition, SheetOptions, SheetTheme};
use borealis_character::Character;
use borealis_engine::CharacterSnapshot;

/// Glyph box above and below the baseline, in em (Noto cap height 0.714, descender ~0.24; the
/// box is the ink a reader sees, so tight but legal leading does not count as overlap).
const ASCENT: f32 = 0.72;
const DESCENT: f32 = 0.2;
/// Rounding slack, in points.
const EPS: f32 = 0.3;

#[derive(Debug, Clone)]
struct TextBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    what: String,
}

impl TextBox {
    fn new(x: f32, base: f32, style: Style, size: f32, text: &str, what: String) -> TextBox {
        TextBox {
            x0: x,
            y0: base - ASCENT * size,
            x1: x + text_width(style, text, size),
            y1: base + DESCENT * size,
            what,
        }
    }

    fn overlaps(&self, o: &TextBox) -> bool {
        self.x0 + EPS < o.x1 && o.x0 + EPS < self.x1 && self.y0 + EPS < o.y1 && o.y0 + EPS < self.y1
    }
}

/// Whether `piece` is the header divider ornament (either art set).
fn is_divider(piece: &ArtPiece) -> bool {
    [ArtSet::Colour, ArtSet::Ink]
        .iter()
        .any(|&set| std::ptr::eq(piece, art::piece(set, ArtId::Divider)))
}

/// Page-space box of an art piece placed at `(x, y)` with `(sx, sy)` points per unit (control
/// points included, so it may be a little generous on curves).
fn art_box(piece: &ArtPiece, (x, y, sx, sy): (f32, f32, f32, f32), what: &str) -> TextBox {
    let mut b = TextBox {
        x0: f32::INFINITY,
        y0: f32::INFINITY,
        x1: f32::NEG_INFINITY,
        y1: f32::NEG_INFINITY,
        what: what.to_string(),
    };
    for p in piece.paths {
        let half = p.stroke.map_or(0.0, |s| s.width / 2.0);
        let pts: Vec<(f32, f32)> = p
            .ops()
            .flat_map(|op| match op {
                Op::Move(a, c) | Op::Line(a, c) => vec![(a, c)],
                Op::Cubic(a, c, d, e, f, g) => vec![(a, c), (d, e), (f, g)],
                Op::Close => vec![],
            })
            .collect();
        for (px, py) in pts {
            let (ax, ay) = (x + px * sx, y + py * sy);
            let (hx, hy) = (half * sx.abs(), half * sy.abs());
            b.x0 = b.x0.min(ax - hx);
            b.x1 = b.x1.max(ax + hx);
            b.y0 = b.y0.min(ay - hy);
            b.y1 = b.y1.max(ay + hy);
        }
    }
    b
}

/// Rules of one page: horizontal and vertical lines, and the header divider ornament.
fn page_rules(ops: &[PageOp]) -> Vec<TextBox> {
    let mut out = Vec::new();
    for op in ops {
        match op {
            PageOp::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                ..
            } => {
                let h = width / 2.0;
                out.push(TextBox {
                    x0: x1.min(*x2) - h,
                    y0: y1.min(*y2) - h,
                    x1: x1.max(*x2) + h,
                    y1: y1.max(*y2) + h,
                    what: format!("rule at y {y1:.1} (x {x1:.1}..{x2:.1})"),
                });
            }
            PageOp::Art {
                piece,
                x,
                y,
                sx,
                sy,
                ..
            } if is_divider(piece) => {
                out.push(art_box(piece, (*x, *y, *sx, *sy), "header divider"));
            }
            _ => {}
        }
    }
    out
}

/// Text boxes of one page, plus fit problems of its field values.
fn page_boxes(ops: &[PageOp]) -> (Vec<TextBox>, Vec<String>) {
    let mut boxes = Vec::new();
    let mut problems = Vec::new();
    for op in ops {
        match op {
            PageOp::Text {
                x,
                y,
                size,
                style,
                text,
                ..
            } if !text.trim().is_empty() => {
                let base = y + BASELINE * size;
                boxes.push(TextBox::new(
                    *x,
                    base,
                    *style,
                    *size,
                    text,
                    format!("text {text:?}"),
                ));
            }
            PageOp::Field(f) => {
                let FieldKind::Text(t) = &f.kind else {
                    continue;
                };
                if t.value.trim().is_empty() {
                    continue;
                }
                let (lines, dropped) = value_lines(t, f.w, f.h);
                if dropped > 0 {
                    problems.push(format!("field {} drops {dropped} wrapped line(s)", f.name));
                }
                for l in lines.iter().filter(|l| !l.text.trim().is_empty()) {
                    let b = TextBox::new(
                        f.x + l.x,
                        f.y + l.base,
                        t.style,
                        t.size,
                        &l.text,
                        format!("field {} {:?}", f.name, l.text),
                    );
                    let inner = (f.x + t.pad - EPS, f.x + f.w - t.pad + EPS);
                    if b.x0 < inner.0 || b.x1 > inner.1 {
                        problems.push(format!(
                            "field {} {:?} is {:.1} pt wide in a {:.1} pt box (pad {})",
                            f.name,
                            l.text,
                            b.x1 - b.x0,
                            f.w,
                            t.pad
                        ));
                    }
                    if b.y0 < f.y - EPS || b.y1 > f.y + f.h + EPS {
                        problems.push(format!(
                            "field {} {:?} leaves its {:.1} pt tall box",
                            f.name, l.text, f.h
                        ));
                    }
                    boxes.push(b);
                }
            }
            _ => {}
        }
    }
    (boxes, problems)
}

/// Every fit and overlap problem of a document's pages.
fn problems(label: &str, pages: &[crate::pdf::PdfPage], (pw, ph): (f32, f32)) -> Vec<String> {
    let mut out = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let (boxes, fit) = page_boxes(&page.ops);
        let at = |p: String| format!("{label} p{}: {p}", i + 1);
        out.extend(fit.into_iter().map(at));
        for b in &boxes {
            if b.x0 < -EPS || b.y0 < -EPS || b.x1 > pw + EPS || b.y1 > ph + EPS {
                out.push(at(format!("{} leaves the page", b.what)));
            }
        }
        for (k, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(k + 1).filter(|b| a.overlaps(b)) {
                out.push(at(format!("{} overlaps {}", a.what, b.what)));
            }
        }
        for r in page_rules(&page.ops) {
            for b in boxes.iter().filter(|b| r.overlaps(b)) {
                out.push(at(format!("{} crosses {}", r.what, b.what)));
            }
        }
    }
    out
}

/// A character with long identity strings, many features and long proficiency lists.
fn stress(mut d: SheetData) -> SheetData {
    d.identity.name = "Seraphina Evangeline Moonwhisper-Vale of the Silver Thorn".into();
    d.identity.class_label = "Wizard 9 / Fighter 6 / Rogue 3 / Cleric 2".into();
    d.identity.subclass_label =
        "School of Evocation, Eldritch Knight, Arcane Trickster, Life Domain".into();
    d.identity.background = "Far Traveler of the Shimmering Outlands".into();
    d.identity.player = "Alexandra Montgomery-Featherstonehaugh".into();
    d.extras.senses = vec![
        "Darkvision 120 ft".into(),
        "Blindsight 30 ft".into(),
        "Tremorsense 15 ft".into(),
        "Truesight 10 ft".into(),
        "Passive Investigation 18".into(),
    ];
    d.proficiencies.languages = [
        "Common",
        "Draconic",
        "Dwarvish",
        "Elvish",
        "Giant",
        "Gnomish",
        "Goblin",
        "Halfling",
        "Orc",
        "Abyssal",
        "Celestial",
        "Deep Speech",
        "Infernal",
        "Primordial",
        "Sylvan",
        "Undercommon",
        "Thieves' Cant",
        "Druidic",
    ]
    .map(String::from)
    .to_vec();
    d.proficiencies.tools = [
        "Alchemist's supplies",
        "Calligrapher's supplies",
        "Cartographer's tools",
        "Thieves' tools",
        "Navigator's tools",
        "Herbalism kit",
        "Disguise kit",
        "Forgery kit",
    ]
    .map(String::from)
    .to_vec();
    let base: Option<FeatureData> = d.features.first().cloned();
    if let Some(f) = base {
        for i in 0..40 {
            d.features.push(FeatureData {
                id: format!("ID_STRESS_{i}"),
                name: format!("Extraordinarily Long Stress Feature Name Number {i}"),
                summary: "A summary long enough to wrap across more than one line of the \
                          feature list, so that the flow has to split and continue."
                    .into(),
                text: "Full text. ".repeat(60),
                ..f.clone()
            });
        }
    }
    d
}

fn options(paper: Paper, ink: bool, fillable: bool, variant: usize) -> SheetOptions {
    SheetOptions {
        paper,
        theme: if ink {
            SheetTheme::Ink
        } else {
            SheetTheme::Colour
        },
        form_fillable: fillable,
        cards: CardKinds {
            spell: variant % 2 == 0,
            item: variant % 2 == 0,
            attack: variant % 2 == 1,
            feature: variant % 2 == 1,
        },
        legacy_spellcasting: variant % 3 == 2,
        ..SheetOptions::default()
    }
}

/// Paper, ink, fillable: each combination of the two values appears, with the card kinds and
/// legacy spell page rotating across them (fillable and flatten share the page ops).
const CONFIGS: [(Paper, bool, bool); 4] = [
    (Paper::Letter, false, true),
    (Paper::A4, true, false),
    (Paper::Letter, true, true),
    (Paper::A4, false, false),
];

fn check(label: &str, pair: &(Character, CharacterSnapshot), stressed: bool) -> Vec<String> {
    let mut out = Vec::new();
    let configs = if stressed {
        &CONFIGS[..]
    } else {
        &CONFIGS[..3]
    };
    for (variant, &(paper, ink, fillable)) in configs.iter().enumerate() {
        let o = options(paper, ink, fillable, variant);
        let mut d =
            crate::sheet_data_for(&pair.0, &pair.1, &SpellFactsMap::new(), SheetEdition::Auto);
        if stressed {
            d = stress(d);
        }
        let (pages, _) =
            build_pages(&d, &pair.0, &pair.1, &o, &SheetContent::default()).expect("pages");
        let name = format!("{label} {paper:?} ink={ink} fillable={fillable}");
        out.extend(problems(&name, &pages, paper.points()));
    }
    out
}

fn assert_clean(found: Vec<String>) {
    // The header repeats on every page: report each problem once, with its first page.
    let mut seen = std::collections::BTreeSet::new();
    let unique: Vec<&String> = found
        .iter()
        .filter(|p| {
            seen.insert(
                p.split_once(": ")
                    .map_or(p.as_str(), |(_, m)| m)
                    .to_string(),
            )
        })
        .collect();
    assert!(
        found.is_empty(),
        "{} fit/overlap problems ({} distinct):\n{}",
        found.len(),
        unique.len(),
        unique
            .iter()
            .take(60)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn fixtures_fit_their_boxes_and_never_overlap() {
    let mut found = Vec::new();
    for (label, pair) in [
        ("wizard_2014", wizard_2014()),
        ("wizard_2024", wizard_2024()),
        ("fighter_wizard_2014", fighter_wizard_2014()),
        ("cleric_wizard_2014", cleric_wizard_2014()),
        ("cleric_2014", cleric_2014()),
    ] {
        found.extend(check(label, &pair, false));
    }
    assert_clean(found);
}

#[test]
fn stress_characters_fit_their_boxes_and_never_overlap() {
    let mut found = Vec::new();
    for (label, pair) in [
        ("stress_2014", wizard_2014()),
        ("stress_2024", wizard_2024()),
    ] {
        found.extend(check(label, &pair, true));
    }
    assert_clean(found);
}

/// Every "continued on page N" note points at the very next page, and the overflowing page 2 of
/// the stress characters carries one.
#[test]
fn continued_notes_point_at_the_next_page() {
    for (label, pair) in [
        ("stress_2014", wizard_2014()),
        ("stress_2024", wizard_2024()),
    ] {
        let o = options(Paper::A4, true, false, 0);
        let mut d =
            crate::sheet_data_for(&pair.0, &pair.1, &SpellFactsMap::new(), SheetEdition::Auto);
        d = stress(d);
        d.story.treasure = "A ledger of debts and maps. ".repeat(40);
        let (pages, _) =
            build_pages(&d, &pair.0, &pair.1, &o, &SheetContent::default()).expect("pages");
        let mut on_page_two = false;
        for (i, page) in pages.iter().enumerate() {
            for op in &page.ops {
                let PageOp::Text { text, .. } = op else {
                    continue;
                };
                let Some(n) = text.strip_prefix("continued on page ") else {
                    continue;
                };
                assert_eq!(n, (i + 2).to_string(), "{label} p{}: {text:?}", i + 1);
                on_page_two |= i == 1;
            }
        }
        assert!(on_page_two, "{label}: page 2 overflows but has no note");
    }
}
