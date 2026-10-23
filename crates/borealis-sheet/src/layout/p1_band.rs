//! The bottom band of the 2014 page 1 (design §4: "P1 adds Personality (traits/ideals/bonds/
//! flaws) ... and equipment with coins"): an equipment summary (carried weight, coins and the
//! first inventory lines, as Aurora's character page prints `details_coinage_*` and the first
//! 15 inventory rows, character-sheet.md §6.1) beside the four personality blocks.
//!
//! [`plan`] measures the band once; page 1 draws it and the Details page carries on any
//! personality text that did not fit (`story.<key>@2`), so both pages agree on the split. The
//! equipment summary is printed text: the editable inventory and coin fields stay on the
//! Inventory page (one widget per id in the document).

use super::grid::{Align, Canvas, Txt, FIELD_PAD};
use super::page_features::features_flow::{body_size, group_digits, trim_num, PITCH};
use super::page_features::features_text::{line_count, split_text};
use crate::fonts::Style;
use crate::model_types::{Edition, SheetData};

/// Heading height (as `Canvas::heading`).
const HEAD_H: f32 = 17.5;
/// Caps label above each personality block.
const LABEL_H: f32 = 8.0;
/// Gap between the equipment half and the personality half.
const GAP: f32 = 10.0;
/// Coin labels and boxes.
const COIN_H: f32 = 25.0;
/// Width of the quantity column of the equipment lines.
const QTY_W: f32 = 18.0;
/// Aurora's character page lists the first 15 inventory rows.
const MAX_ROWS: usize = 15;
/// Fewest equipment lines worth a summary.
const MIN_ROWS: usize = 2;

/// The four personality blocks in page order: `(field id, title)`.
pub(crate) const PERSONALITY: [(&str, &str); 4] = [
    ("story.traits", "Personality traits"),
    ("story.ideals", "Ideals"),
    ("story.bonds", "Bonds"),
    ("story.flaws", "Flaws"),
];

/// One personality block of the band.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Trait {
    pub id: &'static str,
    pub title: &'static str,
    /// Ruled lines on page 1.
    pub lines: usize,
    /// Text printed on page 1.
    pub head: String,
    /// Text that goes on on the Details page.
    pub tail: Option<String>,
}

/// The measured band: `top..bottom` of the page-1 end column.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Band {
    pub top: f32,
    pub bottom: f32,
    pub traits: Vec<Trait>,
}

/// Page numbers the band points at.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BandRefs {
    /// The Details page (personality continued).
    pub details: u32,
    /// The first Inventory page (the full equipment list), when printed.
    pub inventory: Option<u32>,
}

fn story_text<'a>(d: &'a SheetData, id: &str) -> &'a str {
    let s = &d.story;
    match id {
        "story.traits" => &s.traits,
        "story.ideals" => &s.ideals,
        "story.bonds" => &s.bonds,
        _ => &s.flaws,
    }
}

/// `(equipment, personality)` halves of the column `(x, w)`.
fn halves((x, w): (f32, f32)) -> ((f32, f32), (f32, f32)) {
    let hw = (w - GAP) / 2.0;
    ((x, hw), (x + hw + GAP, hw))
}

/// Lines per block: one each, then one more to the block with the most text still unprinted
/// (ties in page order), then any spare lines as write-in room to the shortest block.
fn share_lines(need: &[usize; 4], total: usize) -> [usize; 4] {
    let mut lines = [1usize; 4];
    let mut left = total.saturating_sub(4);
    while left > 0 {
        let best = (0..4)
            .filter(|&i| need[i] > lines[i])
            .max_by_key(|&i| (need[i] - lines[i], std::cmp::Reverse(i)));
        let Some(i) = best else { break };
        lines[i] += 1;
        left -= 1;
    }
    for _ in 0..left {
        let i = (0..4).min_by_key(|&i| (lines[i], i)).unwrap_or(0);
        lines[i] += 1;
    }
    lines
}

/// The band of a 2014 page 1 whose end column has `top..bottom` free, `None` for 2024 or when
/// there is no room for one line per personality block and a short equipment list (page 1 then
/// keeps its Notes and the Details page keeps the personality blocks).
pub(crate) fn plan(d: &SheetData, k: f32, col: (f32, f32), top: f32, bottom: f32) -> Option<Band> {
    if d.edition != Edition::Dnd2014 {
        return None;
    }
    let (pitch, size) = (PITCH * k, body_size(k));
    let h = bottom - top;
    let total = ((h - HEAD_H * k - 4.0 * LABEL_H * k) / pitch).floor();
    let rows = ((h - (HEAD_H + COIN_H) * k) / pitch).floor();
    if total < 4.0 || rows < MIN_ROWS as f32 {
        return None;
    }
    let (_, (_, pw)) = halves(col);
    let width = pw - 2.0 * FIELD_PAD;
    let need = PERSONALITY.map(|(id, _)| {
        let t = story_text(d, id);
        if t.trim().is_empty() {
            0
        } else {
            line_count(Style::Sans, size, width, t)
        }
    });
    let lines = share_lines(&need, total as usize);
    let traits = PERSONALITY
        .iter()
        .zip(lines)
        .map(|(&(id, title), n)| {
            let text = story_text(d, id);
            let (head, tail) = match split_text(Style::Sans, size, width, text, n, 1) {
                Some((h, t)) => (h, Some(t)),
                None => (text.to_string(), None),
            };
            Trait {
                id,
                title,
                lines: n,
                head,
                tail,
            }
        })
        .collect();
    Some(Band {
        top,
        bottom,
        traits,
    })
}

/// Draw the band into the page-1 end column `(x, w)`.
pub(crate) fn draw(c: &mut Canvas, d: &SheetData, band: &Band, col: (f32, f32), refs: BandRefs) {
    let ((ex, ew), (px, pw)) = halves(col);
    equipment(c, d, (ex, band.top, ew), band.bottom, refs.inventory);
    personality(c, band, (px, pw), refs.details);
}

fn personality(c: &mut Canvas, band: &Band, (x, w): (f32, f32), details: u32) {
    let k = c.k;
    let (pitch, size) = (PITCH * k, body_size(k));
    let mut y = c.heading(x, band.top, w, "Personality", None);
    for t in &band.traits {
        let base = y + 5.6 * k;
        c.label(x + 1.0, base, Align::Start, &t.title.to_uppercase());
        if t.tail.is_some() {
            let note = Txt::new(c.s(5.8), Style::SansItalic, c.pal.ink2)
                .align(Align::End)
                .within(w / 2.0);
            c.put(x + w, base, &note, &format!("continued on page {details}"));
        }
        y += LABEL_H * k;
        let h = t.lines as f32 * pitch;
        c.ruled_field(t.id, t.title, (x, y, w, h), &t.head, (pitch, size));
        y += h;
    }
}

fn equipment(
    c: &mut Canvas,
    d: &SheetData,
    (x, y, w): (f32, f32, f32),
    bottom: f32,
    inventory: Option<u32>,
) {
    let k = c.k;
    let g = &d.gear;
    let hint = format!("{} lb carried", trim_num(g.weight));
    let top = c.heading(x, y, w, "Equipment", Some(&hint));
    let gap = 3.0;
    let cw = (w - 4.0 * gap) / 5.0;
    let value = Txt::new(c.s(8.6), Style::SansBold, c.pal.ink).align(Align::Center);
    for (i, label) in ["CP", "SP", "EP", "GP", "PP"].iter().enumerate() {
        let bx = x + i as f32 * (cw + gap);
        c.label(bx + cw / 2.0, top + 5.0 * k, Align::Center, label);
        c.card((bx, top + 7.5 * k, cw, 14.0 * k), 2.5, Some(c.pal.tint));
        c.put(
            bx + cw / 2.0,
            top + 17.6 * k,
            &value.within(cw - 2.0),
            &group_digits(&g.coins[i].to_string()),
        );
    }
    let pitch = PITCH * k;
    let mut row_top = top + COIN_H * k;
    let rows = (((bottom - row_top) / pitch).floor().max(0.0) as usize).min(MAX_ROWS);
    let over = g.items.len().saturating_sub(rows);
    let shown = if over > 0 {
        rows.saturating_sub(1)
    } else {
        g.items.len()
    };
    let hidden = g.items.len() - shown;
    let t = Txt::new(c.s(7.2), Style::Sans, c.pal.ink);
    let qty = t.align(Align::End).within(QTY_W - 3.0);
    let name = t.within(w - QTY_W - 2.0);
    for row in 0..rows {
        let base = row_top + pitch - 3.4 * k;
        if let Some(item) = g.items.get(row).filter(|_| row < shown) {
            c.put(x + QTY_W - 3.0, base, &qty, &item.quantity.to_string());
            c.put(x + QTY_W, base, &name, &item.name);
        } else if hidden > 0 && row == shown {
            let note = Txt::new(c.s(6.4), Style::SansItalic, c.pal.ink2)
                .align(Align::End)
                .within(w);
            let text = match inventory {
                Some(p) => format!("+{hidden} more on page {p}"),
                None => format!("+{hidden} more"),
            };
            c.put(x + w, base, &note, &text);
        }
        row_top += pitch;
        c.hline(x, x + w, row_top, c.pal.rule, 0.5);
    }
}

#[cfg(test)]
#[path = "p1_band_tests.rs"]
mod tests;
