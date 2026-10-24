//! Page chrome: paper, frame, corners, header ribbon, name and meta cells, divider, footer.

use super::art_place::{place, place_clipped};
use super::grid::{Align, Canvas, Grid, Txt, MARGIN_X};
use super::LayoutOptions;
use crate::art::ArtId;
use crate::fonts::{text_width, Style};
use crate::model_ids as id;
use crate::model_types::{Edition, EditionFields, SheetData};
use crate::pdf::{PageOp, PdfPage};
use crate::pdf_draw::{Paint, Path};
use crate::Paper;

const INK_RIBBON_H: f32 = 20.0;
const HEADER_INSET: f32 = 14.0;
const CORNER_SCALE: f32 = 0.5;
const CORNER_AT: (f32, f32) = (14.0, 12.0);
const RIBBON_BOX: (f32, f32, f32, f32) = (24.0, 24.0, 0.0, 70.0);
const NAME_SIZE: f32 = 25.0;
const NAME_BASE: f32 = 62.0;
const IDENT_BASE: f32 = 75.0;
const DIVIDER_Y: f32 = 82.0;
const DIVIDER_SCALE: f32 = 0.6;
/// Width of the divider ornament at [`DIVIDER_SCALE`] (the piece is 400 units wide).
const DIVIDER_W: f32 = 400.0 * DIVIDER_SCALE;
/// Space kept between the divider and the "next" XP note on its right.
const DIVIDER_CLEAR: f32 = 4.0;
/// Gap between the XP value and its "next" note.
const XP_NEXT_W: f32 = 40.0;

/// Paper tone, ribbon, frame and corners: all artifacts (the header draws the divider).
pub(crate) fn furniture(c: &mut Canvas, g: &Grid, o: &LayoutOptions) {
    c.artifact(|c| {
        if !o.ink {
            c.path(Path::rect(0.0, 0.0, g.w, g.h), Paint::fill(c.pal.paper));
        }
        let (rx, ry, _, mut rh) = RIBBON_BOX;
        if o.ink {
            rh = INK_RIBBON_H;
        }
        place_clipped(
            c,
            o.ink,
            ArtId::BorealisRibbon,
            (rx, ry, g.w - 2.0 * rx, rh),
        );
        let frame = if o.paper == Paper::A4 {
            ArtId::FrameA4
        } else {
            ArtId::FrameLetter
        };
        place(c, o.ink, frame, (0.0, 0.0), 1.0);
        let (cx, cy) = CORNER_AT;
        let s = CORNER_SCALE;
        for (x, y, sx, sy) in [
            (cx, cy, s, s),
            (g.w - cx, cy, -s, s),
            (cx, g.h - cy, s, -s),
            (g.w - cx, g.h - cy, -s, -s),
        ] {
            super::art_place::draw_piece(
                c,
                crate::art::piece(super::art_place::set(o.ink), ArtId::Corner),
                (x, y),
                (sx, sy),
            );
        }
    });
}

/// The header divider ornament, centred on the page unless that would cross the "next" XP note
/// whose left edge is `clear_x`: then it moves left of it (and only shrinks when even that does
/// not leave room).
fn divider(c: &mut Canvas, g: &Grid, clear_x: f32) {
    let max_right = clear_x - DIVIDER_CLEAR;
    let left_min = MARGIN_X + HEADER_INSET;
    let centred = (g.w - DIVIDER_W) / 2.0;
    let (x, scale) = if centred + DIVIDER_W <= max_right {
        (centred, DIVIDER_SCALE)
    } else if max_right - DIVIDER_W >= left_min {
        (max_right - DIVIDER_W, DIVIDER_SCALE)
    } else {
        let w = (max_right - left_min).max(DIVIDER_W / 4.0);
        (left_min, DIVIDER_SCALE * w / DIVIDER_W)
    };
    // Keep the ornament's middle on the divider line whatever its scale.
    let y = DIVIDER_Y - 6.0 * scale / DIVIDER_SCALE;
    let ink = c.pal.ink_mode;
    c.artifact(|c| place(c, ink, ArtId::Divider, (x, y), scale));
}

/// One inline piece of the identity line.
enum Seq<'a> {
    Fixed(&'a str, Style),
    Field(&'static str, &'a str, &'a str, Style),
}

fn seq_width(items: &[Seq<'_>], size: f32) -> f32 {
    items
        .iter()
        .map(|i| match i {
            Seq::Fixed(t, st) => text_width(*st, t, size),
            Seq::Field(_, _, v, st) => text_width(*st, v, size),
        })
        .sum()
}

/// Space kept between the identity line and the meta cells, in points.
const IDENT_GAP: f32 = 12.0;
/// Smallest size of the identity line before its longest values are cut with an ellipsis.
const IDENT_MIN: f32 = 7.0;

/// Field values of `items`, the longest cut with an ellipsis until the line fits `max` at `size`.
fn fitted_values(items: &[Seq<'_>], size: f32, max: f32) -> Vec<String> {
    let mut values: Vec<String> = items
        .iter()
        .map(|i| match i {
            Seq::Fixed(..) => String::new(),
            Seq::Field(_, _, v, _) => (*v).to_string(),
        })
        .collect();
    let width = |values: &[String]| -> f32 {
        items
            .iter()
            .zip(values)
            .map(|(i, v)| match i {
                Seq::Fixed(t, st) => text_width(*st, t, size),
                Seq::Field(_, _, _, st) => text_width(*st, v, size),
            })
            .sum()
    };
    // Cut the longest value to what the line can spare; a value with nothing to spare becomes
    // a bare ellipsis and the next longest is cut.
    let mut spent = vec![false; values.len()];
    while width(&values) > max {
        let longest = items
            .iter()
            .zip(&values)
            .enumerate()
            .filter_map(|(k, (i, v))| match i {
                Seq::Field(_, _, _, st) if !v.is_empty() && !spent[k] => {
                    Some((k, text_width(*st, v, size)))
                }
                _ => None,
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((k, w)) = longest else { break };
        let Seq::Field(_, _, _, st) = items[k] else {
            break;
        };
        let target = w - (width(&values) - max);
        let cut = super::grid::truncate(st, &values[k], size, target.max(0.0));
        if cut.is_empty() {
            values[k] = "\u{2026}".into();
            spent[k] = true;
        } else {
            values[k] = cut;
        }
    }
    values
}

/// The identity line: shrinks to [`IDENT_MIN`], then cuts its longest values, so it never
/// runs past `max` (into the meta cells).
fn inline(c: &mut Canvas, x: f32, base: f32, max: f32, size: f32, items: &[Seq<'_>]) {
    let natural = seq_width(items, size);
    let size = if natural > max {
        (size * max / natural).max(IDENT_MIN)
    } else {
        size
    };
    let values = fitted_values(items, size, max);
    let mut cx = x;
    for (item, value) in items.iter().zip(&values) {
        match item {
            Seq::Fixed(t, st) => {
                let col = if *st == Style::Sans || *st == Style::SansItalic {
                    c.pal.ink2
                } else {
                    c.pal.ink
                };
                cx += c.put(cx, base, &Txt::new(size, *st, col), t);
            }
            Seq::Field(fid, tip, _, st) => {
                let w = text_width(*st, value, size) + 2.0;
                let r = (cx - 1.0, base - size, w, size * 1.3);
                c.text_field(fid, tip, r, base, value, &Txt::new(size, *st, c.pal.ink));
                cx += w - 2.0;
            }
        }
    }
}

/// ` (value)` when there is a value; the field exists either way.
fn paren_field<'a>(items: &mut Vec<Seq<'a>>, fid: &'static str, tip: &'a str, value: &'a str) {
    if !value.is_empty() {
        items.push(Seq::Fixed(" (", Style::Sans));
    }
    items.push(Seq::Field(fid, tip, value, Style::Sans));
    if !value.is_empty() {
        items.push(Seq::Fixed(")", Style::Sans));
    }
}

fn group(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Name, identity line and meta cells.
pub(crate) fn header(c: &mut Canvas, g: &Grid, d: &SheetData) {
    let ink = c.pal.ink;
    let right = g.w - MARGIN_X - HEADER_INSET;
    let i = &d.identity;
    // Meta cells, right to left: rules tag, player, XP, level.
    let edition = match d.edition {
        Edition::Dnd2014 => "2014",
        Edition::Dnd2024 => "2024",
    };
    let mut x = right;
    let lab_base = NAME_BASE;
    let val_base = NAME_BASE + 10.0;
    let vsize = 10.0;
    let rules_w = text_width(Style::SerifBold, edition, vsize).max(24.0);
    c.label(x, lab_base, Align::End, "RULES");
    c.put(
        x,
        val_base,
        &Txt::new(vsize, Style::SerifBold, c.pal.teal).align(Align::End),
        edition,
    );
    x -= rules_w + 16.0;
    let player_w = text_width(Style::SerifBold, &i.player, vsize).clamp(30.0, 90.0) + 2.0;
    let cells: [(&str, &str, &str, f32, &str); 3] = [
        ("PLAYER", id::CHAR_PLAYER, &i.player, player_w, "Player"),
        (
            "XP",
            id::CHAR_XP,
            &group(i.experience),
            44.0,
            "Experience points",
        ),
        (
            "LEVEL",
            id::CHAR_LEVEL,
            &i.level.to_string(),
            24.0,
            "Character level",
        ),
    ];
    let mut left_limit = right;
    for (label, fid, value, w, tip) in cells {
        c.label(x, lab_base, Align::End, label);
        let r = (x - w, val_base - vsize, w, vsize * 1.3);
        c.text_field(
            fid,
            tip,
            r,
            val_base,
            value,
            &Txt::new(vsize, Style::SerifBold, ink).align(Align::End),
        );
        left_limit = x - w;
        x -= w + 16.0;
    }
    let t = Txt::new(5.6, Style::SansItalic, c.pal.ink2).align(Align::End);
    let xp_right = right - rules_w - 16.0 - player_w - 16.0;
    let r = (xp_right - XP_NEXT_W, val_base + 3.0, XP_NEXT_W, 7.0);
    let next = i.next_level_xp.map(group).unwrap_or_default();
    c.text_field(
        id::CHAR_XP_NEXT,
        "Experience for next level",
        r,
        val_base + 8.6,
        &next,
        &t,
    );
    let mut clear_x = xp_right - XP_NEXT_W;
    if !next.is_empty() {
        let label_x = xp_right - XP_NEXT_W - 1.0;
        c.put(label_x, val_base + 8.6, &t.align(Align::End), "next");
        clear_x = label_x - text_width(Style::SansItalic, "next", t.size);
    }
    divider(c, g, clear_x);
    // Name.
    let name_max = (left_limit - 24.0 - MARGIN_X - HEADER_INSET).max(80.0);
    let nsize = super::grid::fit_size(Style::SerifBold, &i.name, NAME_SIZE, 14.0, name_max - 2.0);
    let r = (
        MARGIN_X + HEADER_INSET,
        NAME_BASE - nsize,
        name_max,
        nsize * 1.3,
    );
    c.group("H1", |c| {
        c.text_field(
            id::CHAR_NAME,
            "Character name",
            r,
            NAME_BASE,
            &i.name,
            &Txt::new(nsize, Style::SerifBold, ink),
        );
    });
    // Identity line.
    let sub = i.subclass_label.as_str();
    let mut items: Vec<Seq<'_>> = Vec::new();
    let sp = |items: &mut Vec<Seq<'_>>| items.push(Seq::Fixed(" \u{b7} ", Style::Sans));
    let (race_id, race, lineage) = match &d.edition_fields {
        EditionFields::Y2014(f) => (id::RACE, f.race.as_str(), f.subrace.as_deref()),
        EditionFields::Y2024(f) => (id::SPECIES, f.species.as_str(), f.lineage.as_deref()),
    };
    let lin_id = if d.edition == Edition::Dnd2014 {
        id::SUBRACE
    } else {
        id::LINEAGE
    };
    let lin_val = lineage.unwrap_or("");
    items.push(Seq::Field(
        race_id,
        "Race or species",
        race,
        Style::SansBold,
    ));
    paren_field(&mut items, lin_id, "Lineage", lin_val);
    if !i.alignment.is_empty() {
        sp(&mut items);
    }
    items.push(Seq::Field(
        id::CHAR_ALIGNMENT,
        "Alignment",
        &i.alignment,
        Style::Sans,
    ));
    sp(&mut items);
    items.push(Seq::Field(
        id::CHAR_CLASS,
        "Class and level",
        &i.class_label,
        Style::SansBold,
    ));
    paren_field(&mut items, id::CHAR_SUBCLASS, "Subclass", sub);
    sp(&mut items);
    items.push(Seq::Field(
        id::CHAR_BACKGROUND,
        "Background",
        &i.background,
        Style::SansBold,
    ));
    items.push(Seq::Fixed(" background", Style::Sans));
    if let EditionFields::Y2024(f) = &d.edition_fields {
        let feat = f.origin_feat.as_deref().unwrap_or("");
        if !feat.is_empty() {
            sp(&mut items);
        }
        items.push(Seq::Field(
            id::ORIGIN_FEAT,
            "Origin feat",
            feat,
            Style::SansItalic,
        ));
    }
    // The line stops short of the meta cells (level, XP, player, rules) on its right.
    let ident_max =
        (left_limit - IDENT_GAP - MARGIN_X - HEADER_INSET).min(g.content_w() - 2.0 * HEADER_INSET);
    inline(
        c,
        MARGIN_X + HEADER_INSET,
        IDENT_BASE,
        ident_max,
        9.0,
        &items,
    );
}

/// Footer for every page, once the page count is known.
pub(crate) fn footers(pages: &mut [PdfPage], g: &Grid, d: &SheetData, ink: bool) {
    let total = pages.len();
    let edition = if d.edition == Edition::Dnd2014 {
        "2014"
    } else {
        "2024"
    };
    for (n, page) in pages.iter_mut().enumerate() {
        let mut c = Canvas::new(super::grid::Palette::new(ink), false, g.w);
        let col = c.pal.ink2;
        let base = g.h - 40.0;
        let t = Txt::new(6.4, Style::Sans, col);
        c.artifact(|c| {
            c.put(
                MARGIN_X + 20.0,
                base,
                &t,
                &format!("Borealis character sheet \u{b7} {edition} rules"),
            );
            c.put(
                g.w - MARGIN_X - 20.0,
                base,
                &t.align(Align::End),
                &format!("Page {} of {total}", n + 1),
            );
        });
        if n + 1 == total {
            attribution(&mut c, g, d);
        }
        page.ops
            .extend(c.ops.into_iter().filter(|o| !matches!(o, PageOp::Field(_))));
    }
}

/// The SRD CC-BY-4.0 attribution line (last page, below the footer).
fn attribution(c: &mut Canvas, g: &Grid, d: &SheetData) {
    let version = if d.edition == Edition::Dnd2024 {
        "5.2.1"
    } else {
        "5.1"
    };
    let t = Txt::new(5.2, Style::Sans, c.pal.ink2).align(Align::Center);
    let text = format!(
        "Includes material from the System Reference Document {version} by Wizards of the Coast LLC, licensed under CC BY 4.0. Borealis is not affiliated with Wizards of the Coast."
    );
    // Licence text stays readable content (a paragraph), not an artifact.
    c.put(g.w / 2.0, g.h - 31.0, &t, &text);
}
