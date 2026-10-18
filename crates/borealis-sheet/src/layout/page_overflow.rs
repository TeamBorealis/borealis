//! Overflow page after page 1: the attacks and limited uses that did not fit page 1
//! ("+N more on page 2"). Built only when page 1 reports overflow. Ids are the bare
//! `atk.N.*` / `use.N.*` ids of the items (they do not occur on page 1).

use super::chrome;
use super::grid::{Align, Canvas, Grid, Palette, Txt, MARGIN_X};
use super::lists::{ATTACK_CAPACITY, MAX_USE_BOXES, USE_CAPACITY};
use super::LayoutOptions;
use crate::fonts::Style;
use crate::model_ids as id;
use crate::model_types::SheetData;
use crate::pdf::PdfPage;
use crate::pdf_form::{Mark, Shape};

const ROW_H: f32 = 15.0;
const BOX_PITCH: f32 = 9.2;

/// Whether page 1 overflows (more attacks or limited uses than it prints).
pub(crate) fn needed(d: &SheetData) -> bool {
    d.combat.attacks.len() > ATTACK_CAPACITY || d.limited_uses.len() > USE_CAPACITY
}

/// Build the overflow page.
pub(crate) fn build(d: &SheetData, o: &LayoutOptions, g: &Grid) -> PdfPage {
    let mut c = Canvas::new(Palette::new(o.ink), o.compact, g.w - MARGIN_X);
    chrome::furniture(&mut c, g, o);
    chrome::header(&mut c, g, d);
    let (x, w) = (MARGIN_X, g.content_w());
    let mut y = c.heading(x, g.body_top, w, "Attacks & limited uses (continued)", None) + 2.0;
    y = attacks(&mut c, (x, y, w), d, g.body_bottom);
    uses(&mut c, (x, y + 10.0, w), d, g.body_bottom);
    PdfPage {
        ops: c.ops,
        struct_parents: None,
    }
}

fn attacks(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData, bottom: f32) -> f32 {
    let mut y = y;
    let b = Txt::new(c.s(8.4), Style::SansBold, c.pal.ink);
    let s = Txt::new(c.s(7.2), Style::Sans, c.pal.ink);
    let (bonus_r, dmg_x, range_x) = (x + 190.0, x + 200.0, x + 300.0);
    for (i, a) in d.combat.attacks.iter().enumerate().skip(ATTACK_CAPACITY) {
        if y + ROW_H > bottom {
            break;
        }
        let n = i + 1;
        let base = y + ROW_H / 2.0 + 3.0;
        let cell = |dx: f32, cw: f32| (dx, y + 1.0, cw, ROW_H - 2.0);
        c.text_field(
            &id::atk(n, "name"),
            "Attack name",
            cell(x + 1.0, 150.0),
            base,
            &a.name,
            &b,
        );
        c.text_field(
            &id::atk(n, "bonus"),
            "Attack bonus or save DC",
            cell(bonus_r - 36.0, 36.0),
            base,
            &a.bonus,
            &b.align(Align::End),
        );
        c.text_field(
            &id::atk(n, "damage"),
            "Damage",
            cell(dmg_x, 96.0),
            base,
            &a.damage,
            &s,
        );
        let rw = x + w - range_x;
        let half = rw * 0.5;
        c.text_field(
            &id::atk(n, "range"),
            "Range",
            cell(range_x, half - 2.0),
            base,
            &a.range,
            &s,
        );
        c.text_field(
            &id::atk(n, "notes"),
            "Attack notes",
            cell(range_x + half, half),
            base,
            &a.notes,
            &s,
        );
        if a.mastery.is_some() || d.edition == crate::model_types::Edition::Dnd2024 {
            let m = a.mastery.clone().unwrap_or_default();
            c.text_field(
                &id::atk(n, "mastery"),
                "Weapon mastery",
                cell(range_x - 60.0, 56.0),
                base,
                &m,
                &s,
            );
        }
        y += ROW_H;
        c.hline(x, x + w, y, c.pal.rule, 0.5);
    }
    y
}

fn uses(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData, bottom: f32) {
    let mut y = y;
    let bt = Txt::new(c.s(8.0), Style::SansBold, c.pal.ink);
    let rt = Txt::new(c.s(6.8), Style::Sans, c.pal.ink2);
    let boxes_x = x + 170.0;
    let rech_x = x + w - 90.0;
    let per_row = (((rech_x - 6.0 - boxes_x) / BOX_PITCH) as i32).max(4);
    for (i, u) in d.limited_uses.iter().enumerate().skip(USE_CAPACITY) {
        let n = i + 1;
        let drawn = u.max.min(MAX_USE_BOXES);
        let rows = ((drawn + per_row - 1) / per_row).max(1);
        let rh = 12.5 + (rows - 1) as f32 * 9.0;
        if y + rh > bottom {
            break;
        }
        c.text_field(
            &id::use_name(n),
            "Limited use name",
            (x + 1.0, y + 1.0, 160.0, 11.0),
            y + 8.8,
            &u.name,
            &bt,
        );
        for m in 1..=drawn {
            let (col, row) = ((m - 1) % per_row, (m - 1) / per_row);
            let pos = (
                boxes_x + col as f32 * BOX_PITCH,
                y + 2.6 + row as f32 * 9.0,
                7.0,
            );
            c.check(
                &id::use_box(n, m),
                "Use",
                pos,
                Shape::Circle,
                Mark::Fill,
                false,
                true,
            );
        }
        c.put(rech_x, y + 8.6, &rt.within(x + w - rech_x), &u.note());
        y += rh;
        c.hline(x, x + w, y, c.pal.rule, 0.5);
    }
}
