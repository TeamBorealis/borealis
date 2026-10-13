//! Spellcasting page parts: cast strip, slot tracker (class or pooled) and pact slots.

use super::super::grid::{Align, Canvas, Rect, Txt};
use crate::fonts::Style;
use crate::model_ids as id;
use crate::model_types::{CasterData, PactSlots, PooledSlots};
use crate::pdf_draw::signed;
use crate::pdf_form::{Mark, Shape};

/// Height of the cast strip (before compact scaling).
pub(super) const STRIP_H: f32 = 34.0;
/// Height of one pact-magic line.
pub(super) const PACT_H: f32 = 18.0;
/// Height of the slot cells.
const SLOT_H: f32 = 34.0;
/// Gap under the slot cells.
const SLOT_GAP: f32 = 6.0;
/// Heading height before compact scaling.
const HEADING_H: f32 = 17.5;
/// Box size and step of the slot tick boxes.
const BOX: f32 = 7.0;
const BOX_STEP: f32 = 9.4;
/// Cast strip cell widths (relative).
const STRIP_CELLS: [f32; 6] = [100.0, 86.0, 68.0, 68.0, 78.0, 100.0];
const STRIP_GAP: f32 = 5.6;

/// Which field ids a slot table uses.
#[derive(Clone, Copy)]
pub(super) enum SlotIds {
    Class(usize),
    Pool,
}

impl SlotIds {
    fn total(self, level: usize) -> String {
        match self {
            SlotIds::Class(n) => id::spell_slot_total(n, level),
            SlotIds::Pool => id::pool_slot_total(level),
        }
    }

    fn used(self, level: usize, k: i32) -> String {
        match self {
            SlotIds::Class(n) => id::spell_slot_used(n, level, k),
            SlotIds::Pool => id::pool_slot_used(level, k),
        }
    }
}

/// Height of a slot table with its heading.
pub(super) fn slots_height(k: f32) -> f32 {
    (HEADING_H + SLOT_H) * k + SLOT_GAP
}

/// The cast strip of a caster; returns its height.
pub(super) fn strip(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    n: usize,
    k: &CasterData,
    sfx: &str,
) -> f32 {
    let h = c.s(STRIP_H);
    let prepared = if k.prepares {
        match k.prepare_limit {
            Some(limit) => format!("{} / {limit}", k.prepared_count),
            None => k.prepared_count.to_string(),
        }
    } else {
        format!("{} known", k.cantrips.len() + k.spells.len())
    };
    let prepared_label = if k.prepares { "PREPARED" } else { "SPELLS" };
    let cells: [(&str, &str, &str, String, f32); 6] = [
        (
            "CLASS",
            "class",
            "Spellcasting class",
            k.class.clone(),
            11.0,
        ),
        (
            "CASTING ABILITY",
            "ability",
            "Spellcasting ability",
            k.ability_name.clone(),
            11.0,
        ),
        (
            "MODIFIER",
            "mod",
            "Spellcasting modifier",
            signed(i64::from(k.ability_modifier)),
            15.0,
        ),
        ("SAVE DC", "dc", "Spell save DC", k.dc.to_string(), 15.0),
        (
            "SPELL ATTACK",
            "attack",
            "Spell attack bonus",
            signed(i64::from(k.attack)),
            15.0,
        ),
        (
            prepared_label,
            "prepared",
            "Spells prepared",
            prepared,
            11.0,
        ),
    ];
    let total: f32 = STRIP_CELLS.iter().sum();
    let avail = w - STRIP_GAP * 5.0;
    let mut cx = x;
    for ((label, part, tip, value, size), rel) in cells.into_iter().zip(STRIP_CELLS) {
        let cw = avail * rel / total;
        c.card((cx, y, cw, h), 3.0, None);
        c.label(cx + cw / 2.0, y + c.s(10.0), Align::Center, label);
        let t = Txt::new(c.s(size), Style::SerifBold, c.pal.ink).align(Align::Center);
        let r: Rect = (cx + 3.0, y + c.s(13.0), cw - 6.0, c.s(18.0));
        let base = y + c.s(27.5);
        c.text_field(
            &format!("{}{sfx}", id::spell_head(n, part)),
            tip,
            r,
            base,
            &value,
            &t,
        );
        cx += cw + STRIP_GAP;
    }
    h
}

/// A slot tracker for levels 1 to 9; unused levels are faint. `extra` adds a text field (the
/// pooled caster level) at the end of the heading row. Returns the height.
pub(super) fn slot_table(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    ids: SlotIds,
    slots: [i32; 9],
    heading: (&str, &str, Option<(String, String)>),
    sfx: &str,
) -> f32 {
    let (title, hint, extra) = heading;
    let head_w = if extra.is_some() { w - 96.0 } else { w };
    let mut top = c.heading(x, y, head_w, title, Some(hint));
    if let Some((fid, value)) = extra {
        let base = y + c.s(9.4);
        c.label(x + w - 28.0, base, Align::End, "CASTER LEVEL");
        let t = Txt::new(c.s(8.0), Style::SansBold, c.pal.ink).align(Align::Center);
        let r: Rect = (x + w - 25.0, y + 0.5, 25.0, 12.0);
        c.text_field(
            &format!("{fid}{sfx}"),
            "Combined caster level",
            r,
            base,
            &value,
            &t,
        );
    }
    let gap = 4.0;
    let cw = (w - 8.0 * gap) / 9.0;
    let ch = c.s(SLOT_H);
    top += 0.0;
    for level in 1..=9usize {
        let total = slots[level - 1].max(0);
        let cx = x + (level - 1) as f32 * (cw + gap);
        let active = total > 0;
        let fill = active.then_some(c.pal.tint);
        c.card((cx, top, cw, ch), 3.0, fill);
        let col = if active { c.pal.ink } else { c.pal.rule };
        let lt = Txt::new(c.s(8.4), Style::SerifBold, col);
        c.put(cx + 5.0, top + c.s(11.0), &lt, &level.to_string());
        let tcol = if active { c.pal.ink2 } else { c.pal.rule };
        let tt = Txt::new(c.s(7.0), Style::SansBold, tcol).align(Align::End);
        let r: Rect = (cx + cw - 24.0, top + 2.5, 20.0, 10.0);
        let fid = format!("{}{sfx}", ids.total(level));
        c.text_field(
            &fid,
            &format!("Level {level} slots"),
            r,
            top + c.s(10.6),
            &total.to_string(),
            &tt,
        );
        tick_row(c, (cx, top + c.s(17.0), cw), total, |k| {
            format!("{}{sfx}", ids.used(level, k))
        });
    }
    c.s(HEADING_H) + ch + SLOT_GAP
}

/// Tick boxes `1..=total` centred in a cell; wraps when a cell is too narrow.
fn tick_row(c: &mut Canvas, (cx, y, cw): (f32, f32, f32), total: i32, fid: impl Fn(i32) -> String) {
    let per_row = (((cw - 6.0) / BOX_STEP) as i32).max(1);
    for k in 1..=total {
        let (col, row) = ((k - 1) % per_row, (k - 1) / per_row);
        let in_row = (total - row * per_row).min(per_row);
        let left = cx + (cw - ((in_row as f32) * BOX_STEP - (BOX_STEP - BOX))) / 2.0;
        let (bx, by) = (left + col as f32 * BOX_STEP, y + row as f32 * BOX_STEP);
        c.check(
            &fid(k),
            "Slot expended",
            (bx, by, BOX),
            Shape::Circle,
            Mark::Fill,
            false,
            true,
        );
    }
}

/// The pooled multiclass slot table.
pub(super) fn pooled(c: &mut Canvas, rect: (f32, f32, f32), pool: PooledSlots, sfx: &str) -> f32 {
    let extra = Some((id::POOL_LEVEL.to_string(), pool.caster_level.to_string()));
    let hint = "all spellcasting classes combined \u{b7} tick when expended";
    slot_table(
        c,
        rect,
        SlotIds::Pool,
        pool.slots,
        ("Spell slots", hint, extra),
        sfx,
    )
}

/// One pact-magic line: slot count, level and tick boxes.
pub(super) fn pact(
    c: &mut Canvas,
    (x, y, _w): (f32, f32, f32),
    n: usize,
    p: PactSlots,
    sfx: &str,
) -> f32 {
    let base = y + c.s(10.0);
    let bold = Txt::new(c.s(7.4), Style::SansBold, c.pal.ink);
    let plain = Txt::new(c.s(6.8), Style::Sans, c.pal.ink2);
    c.put(x + 2.0, base, &bold, "Pact magic");
    let nt = Txt::new(c.s(8.0), Style::SansBold, c.pal.ink).align(Align::Center);
    let f = |part: &str| format!("{}{sfx}", id::spell_pact(n, part));
    c.text_field(
        &f("total"),
        "Pact slots",
        (x + 56.0, y + 1.0, 18.0, 12.0),
        base,
        &p.count.to_string(),
        &nt,
    );
    c.put(x + 78.0, base, &plain, "slots of level");
    c.text_field(
        &f("level"),
        "Pact slot level",
        (x + 124.0, y + 1.0, 18.0, 12.0),
        base,
        &p.level.to_string(),
        &nt,
    );
    for k in 1..=p.count {
        let bx = x + 152.0 + (k - 1) as f32 * BOX_STEP;
        let fid = format!("{}{sfx}", id::spell_pact_used(n, k));
        c.check(
            &fid,
            "Pact slot expended",
            (bx, y + 3.0, BOX),
            Shape::Circle,
            Mark::Fill,
            false,
            true,
        );
    }
    let after = x + 152.0 + p.count.max(0) as f32 * BOX_STEP + 6.0;
    let it = Txt::new(c.s(6.4), Style::SansItalic, c.pal.ink2);
    c.put(after, base, &it, "refill on a short rest");
    c.s(PACT_H)
}
