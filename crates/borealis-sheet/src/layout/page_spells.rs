//! Spellcasting page(s) with continuation.
//!
//! One block per caster (cast strip, slot tracker, spell table split by level). A single pooled
//! slot table replaces the per-class trackers of multiclass casters. Lists continue on pages
//! titled "Spellcasting (continued)" with a repeated column header and "Nth level (cont.)".

#[path = "spells_slots.rs"]
mod spells_slots;
#[path = "spells_table.rs"]
mod spells_table;
pub(super) use spells_table::pip_shape;
#[cfg(test)]
#[path = "spells_tests.rs"]
mod spells_tests;

use super::chrome;
use super::grid::{Align, Canvas, Grid, Palette, Txt, MARGIN_X};
use super::LayoutOptions;
use crate::fonts::Style;
use crate::model_types::{CasterData, SheetData, SpellRow};
use crate::pdf::PdfPage;
use spells_slots::SlotIds;

/// Space kept free above the body bottom for the "continued" note.
const NOTE_RESERVE: f32 = 11.0;
/// Gap between blocks.
const BLOCK_GAP: f32 = 7.0;

/// A built page and the width limit of every text op (for tests).
pub(super) type Built = (PdfPage, Vec<(usize, f32)>);

/// Page-by-page builder: owns the current canvas and the vertical cursor.
struct Pager<'a> {
    d: &'a SheetData,
    o: &'a LayoutOptions,
    g: &'a Grid,
    first_no: u32,
    done: Vec<Built>,
    c: Canvas,
    y: f32,
    idx: usize,
}

impl<'a> Pager<'a> {
    fn new(d: &'a SheetData, o: &'a LayoutOptions, g: &'a Grid, first_no: u32) -> Pager<'a> {
        let mut p = Pager {
            d,
            o,
            g,
            first_no,
            done: Vec::new(),
            c: Canvas::new(Palette::new(o.ink), o.compact, g.w - MARGIN_X),
            y: 0.0,
            idx: 0,
        };
        p.begin();
        p
    }

    /// Chrome and title of the current page.
    fn begin(&mut self) {
        self.c = Canvas::new(
            Palette::new(self.o.ink),
            self.o.compact,
            self.g.w - MARGIN_X,
        );
        chrome::furniture(&mut self.c, self.g, self.o);
        chrome::header(&mut self.c, self.g, self.d);
        let title = if self.idx == 0 {
            "Spellcasting"
        } else {
            "Spellcasting (continued)"
        };
        self.y = self
            .c
            .heading(MARGIN_X, self.g.body_top, self.g.content_w(), title, None)
            + 2.0;
    }

    /// Id suffix of the current page: none on the first spell page, then `@2`, `@3`...
    fn sfx(&self) -> String {
        if self.idx == 0 {
            String::new()
        } else {
            format!("@{}", self.idx + 1)
        }
    }

    fn room(&self) -> f32 {
        self.g.body_bottom - NOTE_RESERVE - self.y
    }

    fn rect(&self) -> (f32, f32, f32) {
        (MARGIN_X, self.y, self.g.content_w())
    }

    /// Start a new page when `h` does not fit. Returns whether it did.
    fn ensure(&mut self, h: f32) -> bool {
        if h <= self.room() {
            return false;
        }
        self.next_page();
        true
    }

    fn next_page(&mut self) {
        let note = format!("Continued on page {}", self.first_no + self.idx as u32 + 1);
        self.c.close_table();
        let t = Txt::new(6.4, Style::SansItalic, self.c.pal.ink2).align(Align::End);
        self.c
            .put(self.g.w - MARGIN_X, self.g.body_bottom - 2.0, &t, &note);
        let page = PdfPage {
            ops: std::mem::take(&mut self.c.ops),
            struct_parents: None,
        };
        self.done.push((page, std::mem::take(&mut self.c.limits)));
        self.idx += 1;
        self.begin();
    }

    fn finish(mut self) -> Vec<Built> {
        self.c.close_table();
        let page = PdfPage {
            ops: std::mem::take(&mut self.c.ops),
            struct_parents: None,
        };
        self.done.push((page, std::mem::take(&mut self.c.limits)));
        self.done
    }
}

/// Height of a caster's head (strip, slots, pact, table heading and first rows).
fn head_height(c: &CasterData, k: f32) -> f32 {
    let mut h = spells_slots::STRIP_H * k + BLOCK_GAP;
    if !c.slots_pooled {
        h += spells_slots::slots_height(k);
    }
    if c.pact.is_some() {
        h += spells_slots::PACT_H * k;
    }
    h + spells_table::intro_height(k) + spells_table::LEVEL_H + spells_table::ROW_H
}

/// A listed row with its index in the full `cantrips + spells` order (the field id index).
fn listed_rows(c: &CasterData, include_unprepared: bool) -> Vec<(usize, &SpellRow)> {
    let cantrips = c.cantrips.iter().enumerate().map(|(i, r)| (i + 1, r));
    let spells = c.listed(include_unprepared).into_iter().filter_map(|r| {
        let at = c.spells.iter().position(|s| std::ptr::eq(s, r))?;
        Some((c.cantrips.len() + at + 1, r))
    });
    cantrips.chain(spells).collect()
}

fn caster_block(p: &mut Pager<'_>, n: usize, c: &CasterData, include_unprepared: bool) {
    let k = p.c.k;
    p.ensure(head_height(c, k));
    let sfx = p.sfx();
    let r = p.rect();
    p.y += spells_slots::strip(&mut p.c, r, n, c, &sfx) + BLOCK_GAP;
    if !c.slots_pooled {
        let ids = SlotIds::Class(n);
        let head = ("Spell slots", "tick when expended", None);
        let r = p.rect();
        p.y += spells_slots::slot_table(&mut p.c, r, ids, c.slots, head, &sfx);
    }
    if let Some(pact) = c.pact {
        let r = p.rect();
        p.y += spells_slots::pact(&mut p.c, r, n, pact, &sfx);
    }
    let r = p.rect();
    p.y += spells_table::intro(&mut p.c, r);
    let rows = listed_rows(c, include_unprepared);
    let pooled = p.d.spellcasting.pooled.map(|s| s.slots);
    let slots = if c.slots_pooled {
        pooled
    } else {
        Some(c.slots)
    };
    let mut current: Option<u32> = None;
    let mut fresh = false;
    if rows.is_empty() {
        let r = p.rect();
        p.y += spells_table::empty(&mut p.c, r);
    }
    for (index, row) in rows {
        let new_level = current != Some(row.level);
        let header_h = if new_level || fresh {
            spells_table::LEVEL_H
        } else {
            0.0
        };
        if p.ensure(header_h + spells_table::ROW_H) {
            let r = p.rect();
            p.y += spells_table::continued_head(&mut p.c, r, &c.class);
            fresh = true;
        }
        if new_level || fresh {
            let sub = if new_level { slots } else { None };
            let r = p.rect();
            p.y += p.c.group("TR", |c| {
                spells_table::level_header(c, r, row.level, !new_level, sub)
            });
        }
        fresh = false;
        current = Some(row.level);
        let sfx = p.sfx();
        let r = p.rect();
        p.y += p.c.group("TR", |cv| {
            spells_table::row(cv, r, (n, index), (row, c), &sfx)
        });
    }
    p.c.close_table();
    p.y += BLOCK_GAP;
}

/// Build every spellcasting page (none for a non-caster). `first_page_no` is the 1-based number
/// of the first spellcasting page (for the "Continued on page N" note).
pub(crate) fn build(
    data: &SheetData,
    opts: &LayoutOptions,
    include_unprepared: bool,
    grid: &Grid,
    first_page_no: u32,
) -> Vec<PdfPage> {
    build_with_limits(data, opts, include_unprepared, grid, first_page_no)
        .into_iter()
        .map(|(page, _)| page)
        .collect()
}

pub(super) fn build_with_limits(
    data: &SheetData,
    opts: &LayoutOptions,
    include_unprepared: bool,
    grid: &Grid,
    first_page_no: u32,
) -> Vec<Built> {
    let sc = &data.spellcasting;
    if sc.casters.is_empty() {
        return Vec::new();
    }
    let mut p = Pager::new(data, opts, grid, first_page_no);
    if let Some(pool) = sc.pooled {
        let k = p.c.k;
        p.ensure(spells_slots::slots_height(k));
        let sfx = p.sfx();
        let r = p.rect();
        p.y += spells_slots::pooled(&mut p.c, r, pool, &sfx) + BLOCK_GAP;
    }
    for (i, c) in sc.casters.iter().enumerate() {
        caster_block(&mut p, i + 1, c, include_unprepared);
    }
    p.finish()
}
