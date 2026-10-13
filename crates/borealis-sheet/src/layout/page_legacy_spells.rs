//! The basic ("legacy") spellcasting page: one page per caster with fixed row
//! capacities, redistributed when a level has more spells than
//! rows (see [`super::legacy_plan`]). Only a list too long for the page at the smallest row
//! height loses spells, and the page says how many ("+N more spells not shown").
//! Every row is a name field plus a prepared mark, so a fillable sheet doubles as a write-in page.
//! Field ids: `legacy.<caster>.<level>.<row>.name` and `.prep` (all 1-based except the level).

use super::chrome;
use super::grid::{Align, Canvas, Grid, Palette, Rect, Txt, MARGIN_X};
use super::legacy_plan::{dropped_note, level_title, plan, Seg, LEVEL_GAP, LEVEL_H};
use super::LayoutOptions;
use crate::fonts::Style;
use crate::model_types::{CasterData, SheetData, SpellRow};
use crate::pdf::PdfPage;
use crate::pdf_form::{Mark, Shape};

const COL_GAP: f32 = 10.0;
const STRIP_H: f32 = 28.0;
const CELL_GAP: f32 = 6.0;
const PIP: f32 = 7.0;
const NAME_SIZE: f32 = 7.2;
/// Space kept above the body bottom for the "not shown" note.
const NOTE_H: f32 = 12.0;
/// Height of the "Spellcasting (basic)" heading (see `Canvas::heading`) plus its gap.
const HEADING_H: f32 = 17.5;
const HEADING_GAP: f32 = 2.0;
const STRIP_GAP: f32 = 8.0;

fn fid(caster: usize, level: usize, row: usize, part: &str) -> String {
    format!("legacy.{caster}.{level}.{row}.{part}")
}

/// Top of the level columns.
fn columns_top(g: &Grid, k: f32) -> f32 {
    g.body_top + HEADING_H * k + HEADING_GAP + STRIP_H * k + STRIP_GAP
}

/// Height of the level columns (above the note line).
pub(crate) fn columns_avail(g: &Grid, compact: bool) -> f32 {
    let k = if compact { super::grid::COMPACT_K } else { 1.0 };
    g.body_bottom - NOTE_H - columns_top(g, k)
}

fn strip(c: &mut Canvas, x: f32, y: f32, w: f32, caster: &CasterData) {
    let k = c.k;
    let cells = [
        ("CLASS", caster.class.clone()),
        ("ABILITY", caster.ability_name.clone()),
        ("SPELL SAVE DC", caster.dc.to_string()),
        ("SPELL ATTACK", format!("{:+}", caster.attack)),
    ];
    let cw = (w - 3.0 * CELL_GAP) / 4.0;
    let tint = c.pal.tint;
    for (i, (label, value)) in cells.iter().enumerate() {
        let cx = x + i as f32 * (cw + CELL_GAP);
        c.card((cx, y, cw, STRIP_H * k), 3.0, Some(tint));
        c.label(cx + cw / 2.0, y + 8.5 * k, Align::Center, label);
        let t = Txt::new(11.0 * k, Style::SerifBold, c.pal.ink)
            .align(Align::Center)
            .within(cw - 8.0);
        c.put(cx + cw / 2.0, y + 22.0 * k, &t, value);
    }
}

fn level_header(c: &mut Canvas, (x, y, w): (f32, f32, f32), seg: Seg, slots: Option<i32>) {
    let title = level_title(seg);
    let base = y + 9.5;
    c.put(x, base, &Txt::new(8.4, Style::SerifBold, c.pal.ink), &title);
    if let Some(n) = slots.filter(|n| *n > 0) {
        let noun = if n == 1 { "slot" } else { "slots" };
        let t = Txt::new(6.2, Style::SansItalic, c.pal.ink2).align(Align::End);
        c.put(x + w, base, &t, &format!("{n} {noun}"));
    }
    c.hline(x, x + w, y + LEVEL_H - 1.5, c.pal.ink2, 0.6);
}

fn pip(row: Option<&SpellRow>) -> (Shape, bool) {
    match row {
        Some(r) => super::page_spells::pip_shape(r),
        None => (Shape::Circle, false),
    }
}

fn spell_row(
    c: &mut Canvas,
    (x, y, w, h): Rect,
    (n, level, index): (usize, usize, usize),
    row: Option<&SpellRow>,
) {
    let (shape, checked) = pip(row);
    c.check(
        &fid(n, level, index, "prep"),
        "Prepared",
        (x, y + (h - PIP) / 2.0, PIP),
        shape,
        Mark::Fill,
        checked,
        !checked,
    );
    let name = row.map_or("", |r| r.name.as_str());
    let t = Txt::new(NAME_SIZE, Style::Sans, c.pal.ink);
    c.text_field(
        &fid(n, level, index, "name"),
        "Spell name",
        (x + PIP + 3.0, y, w - PIP - 3.0, h),
        y + h - 3.2,
        name,
        &t,
    );
    c.hline(x + PIP + 3.0, x + w, y + h, c.pal.rule, 0.5);
}

fn page(
    data: &SheetData,
    opts: &LayoutOptions,
    g: &Grid,
    n: usize,
    caster: &CasterData,
    include_unprepared: bool,
) -> PdfPage {
    let mut c = Canvas::new(Palette::new(opts.ink), opts.compact, g.w - MARGIN_X);
    chrome::furniture(&mut c, g, opts);
    chrome::header(&mut c, g, data);
    let w = g.content_w();
    let y = c.heading(
        MARGIN_X,
        g.body_top,
        w,
        "Spellcasting (basic)",
        Some(&caster.class),
    ) + HEADING_GAP;
    strip(&mut c, MARGIN_X, y, w, caster);
    let y = columns_top(g, c.k);
    let p = plan(caster, include_unprepared, columns_avail(g, opts.compact));
    let colw = (w - 2.0 * COL_GAP) / 3.0;
    for (ci, levels) in p.columns.iter().enumerate() {
        let x = MARGIN_X + ci as f32 * (colw + COL_GAP);
        let mut cy = y;
        for (li, &seg) in levels.iter().enumerate() {
            if li > 0 {
                cy += LEVEL_GAP;
            }
            let level = seg.level;
            let slots =
                (level > 0 && !caster.slots_pooled && !seg.cont).then(|| caster.slots[level - 1]);
            level_header(&mut c, (x, cy, colw), seg, slots);
            cy += LEVEL_H;
            for i in seg.first..seg.first + seg.count {
                let row = p.by[level].get(i).copied();
                spell_row(&mut c, (x, cy, colw, p.row_h), (n, level, i + 1), row);
                cy += p.row_h;
            }
        }
    }
    let note = dropped_note(p.dropped);
    if !note.is_empty() {
        let t = Txt::new(6.6, Style::SansItalic, c.pal.ink).align(Align::End);
        c.put(g.w - MARGIN_X, g.body_bottom - 2.0, &t, &note);
    }
    PdfPage {
        ops: c.ops,
        struct_parents: None,
    }
}

/// One basic page per caster (none for a non-caster).
pub(crate) fn build(
    data: &SheetData,
    opts: &LayoutOptions,
    include_unprepared: bool,
    g: &Grid,
) -> Vec<PdfPage> {
    data.spellcasting
        .casters
        .iter()
        .enumerate()
        .map(|(i, caster)| page(data, opts, g, i + 1, caster, include_unprepared))
        .collect()
}
