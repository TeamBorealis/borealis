//! Gear blocks: attunement slots, coins, carrying, and the first-page equipment layout.

use super::features_flow::{
    blank_rows, fill, fill_last, group_digits, trim_num, ContMark, Cx, Piece, Region, BLOCK_GAP,
    COLHEAD_H, HEAD_H, PITCH, ROW_H,
};
use crate::fonts::Style;
use crate::layout::grid::{Align, Txt};
use crate::model_ids as id;
use crate::pdf_form::{Mark, Shape};
use std::collections::VecDeque;

const ATTUNE_ROW_H: f32 = 17.0;
const COIN_BOX_H: f32 = 18.0;
const CARRY_ROW_H: f32 = 15.0;
const SECTION_GAP: f32 = 6.0;

fn attune_h(k: f32, slots: u32) -> f32 {
    if slots == 0 {
        0.0
    } else {
        (HEAD_H + ATTUNE_ROW_H * slots as f32 + SECTION_GAP) * k
    }
}

fn coins_h(k: f32) -> f32 {
    (HEAD_H + 8.0 + COIN_BOX_H + SECTION_GAP) * k
}

fn carry_h(k: f32) -> f32 {
    (HEAD_H + 2.0 * CARRY_ROW_H + SECTION_GAP) * k
}

/// Height of attunement, coins and carrying together.
pub(crate) fn side_h(k: f32, slots: u32) -> f32 {
    attune_h(k, slots) + coins_h(k) + carry_h(k)
}

/// Attunement slots; returns the y below.
pub(crate) fn attunement(cx: &mut Cx, (x, y, w): (f32, f32, f32)) -> f32 {
    let k = cx.c.k;
    let slots = cx.d.gear.attunement_limit;
    if slots == 0 {
        return y;
    }
    let hint = format!("{} / {}", cx.d.gear.attuned.len(), slots);
    let mut top = cx.c.heading(x, y, w, "Magic item attunement", Some(&hint));
    let t = Txt::new(8.0 * k, Style::Sans, cx.c.pal.ink);
    for n in 1..=slots as usize {
        let cid = cx.ids.make(&id::attune(n), cx.ordinal);
        cx.c.check(
            &cid,
            "Attuned",
            (x + 1.0, top + 2.0, 11.0 * k),
            Shape::Diamond,
            Mark::Fill,
            false,
            true,
        );
        let nid = cx.ids.make(&id::attune_name(n), cx.ordinal);
        let name = cx.d.gear.attuned.get(n - 1).cloned().unwrap_or_default();
        cx.c.tinted_field(
            &nid,
            "Attuned item",
            (x + 18.0, top + 1.5, w - 18.0, 13.0 * k),
            top + 10.8 * k,
            &name,
            &t,
        );
        top += ATTUNE_ROW_H * k;
    }
    top + SECTION_GAP * k
}

/// Coins CP SP EP GP PP; returns the y below.
pub(crate) fn coins(cx: &mut Cx, (x, y, w): (f32, f32, f32)) -> f32 {
    let k = cx.c.k;
    let top = cx.c.heading(x, y, w, "Coins", None);
    let gap = 5.0;
    let cw = (w - 4.0 * gap) / 5.0;
    let t = Txt::new(11.0 * k, Style::SansBold, cx.c.pal.ink).align(Align::Center);
    for (i, label) in ["CP", "SP", "EP", "GP", "PP"].iter().enumerate() {
        let bx = x + i as f32 * (cw + gap);
        cx.c.label(bx + cw / 2.0, top + 5.0 * k, Align::Center, label);
        let fid = cx.ids.make(id::COINS[i], cx.ordinal);
        cx.c.tinted_field(
            &fid,
            label,
            (bx, top + 8.0 * k, cw, COIN_BOX_H * k),
            top + 8.0 * k + 13.4 * k,
            &group_digits(&cx.d.gear.coins[i].to_string()),
            &t.within(cw - 2.0),
        );
    }
    top + (8.0 + COIN_BOX_H + SECTION_GAP) * k
}

/// Carried weight and capacity; returns the y below.
pub(crate) fn carry(cx: &mut Cx, (x, y, w): (f32, f32, f32)) -> f32 {
    let k = cx.c.k;
    let mut top = cx.c.heading(x, y, w, "Carrying", None);
    let t = Txt::new(8.4 * k, Style::SansBold, cx.c.pal.ink);
    let g = &cx.d.gear;
    let over = g.capacity > 0.0 && g.weight > g.capacity;
    let carried = format!("{} lb", trim_num(g.weight));
    let rows = [
        (
            "CARRIED",
            id::CARRY_WEIGHT,
            if over {
                format!("{carried} \u{b7} over")
            } else {
                carried
            },
        ),
        (
            "CAPACITY",
            id::CARRY_CAPACITY,
            format!("{} lb", trim_num(g.capacity)),
        ),
    ];
    for (label, fid, value) in rows {
        cx.c.label(x + 1.0, top + 9.6 * k, Align::Start, label);
        let fid = cx.ids.make(fid, cx.ordinal);
        cx.c.tinted_field(
            &fid,
            label,
            (x + 64.0, top + 1.0, 78.0, 12.5 * k),
            top + 10.0 * k,
            &value,
            &t,
        );
        top += CARRY_ROW_H * k;
    }
    top + SECTION_GAP * k
}

/// Equipment piece queue: heading with column labels, then one row per item from `from`.
pub(crate) fn equipment_queue(
    cx: &Cx,
    from: usize,
    title: &str,
    hint: Option<String>,
) -> VecDeque<Piece> {
    let mut q = VecDeque::new();
    q.push_back(Piece::EquipHead {
        title: title.to_string(),
        hint,
    });
    for i in from..cx.d.gear.items.len() {
        q.push_back(Piece::EquipRow(i));
    }
    q
}

fn region(cx: &Cx, key: usize, min_lines: usize, fill: bool) -> Piece {
    let s = &cx.d.story;
    let (idv, title, text, tip) = match key {
        0 => (
            "story.equipment_notes",
            "Equipment notes",
            &s.equipment_notes,
            "Equipment notes",
        ),
        1 => ("story.treasure", "Treasure", &s.treasure, "Treasure"),
        _ => ("story.quest", "Quest items", &s.quest, "Quest items"),
    };
    Piece::Region(Region {
        id: idv.to_string(),
        tip: tip.to_string(),
        title: title.to_string(),
        text: text.clone(),
        min_lines,
        fill,
    })
}

/// Pieces for the equipment table to continue on later pages: a "(continued)" heading when rows
/// remain.
pub(crate) fn continue_equipment(q: &mut VecDeque<Piece>) {
    if matches!(q.front(), Some(Piece::EquipRow(_))) {
        q.push_front(Piece::EquipHead {
            title: "Equipment (continued)".to_string(),
            hint: None,
        });
    }
}

/// Baseline of the "continued on page N" note under a cut equipment table (in the section gap).
const TABLE_MARK_BASE: f32 = 4.8;

/// The 2024 end column of page 2: equipment table, attunement, coins, carrying, notes, treasure
/// and quest items. Returns what did not fit (for the continuation pages); `marks` gets the
/// "continued on page N" notes of the blocks that go on.
pub(crate) fn column_2024(
    cx: &mut Cx,
    (x, y0, w): (f32, f32, f32),
    bottom: f32,
    marks: &mut Vec<ContMark>,
) -> VecDeque<Piece> {
    let k = cx.c.k;
    let slots = cx.d.gear.attunement_limit;
    let tail_min = 3.0 * (HEAD_H + BLOCK_GAP) * k + 7.0 * PITCH * k;
    let table_max = bottom - y0 - side_h(k, slots) - tail_min;
    let head = (HEAD_H + COLHEAD_H) * k;
    let cap = (((table_max - head) / (ROW_H * k)).floor() as i64).max(1) as usize;
    let rows = (cx.d.gear.items.len() + 3).max(10).min(cap);
    let table_bottom = y0 + head + rows as f32 * ROW_H * k;
    let hint = format!("{} lb carried", trim_num(cx.d.gear.weight));
    let mut q = equipment_queue(cx, 0, "Equipment", Some(hint));
    let yy = fill(cx, &mut q, (x, y0, w), table_bottom);
    if q.is_empty() {
        blank_rows(cx.c, (x, yy, w), table_bottom);
    } else {
        marks.push(ContMark {
            page: cx.page_idx,
            x_right: x + w,
            base: table_bottom + TABLE_MARK_BASE * k,
        });
    }
    continue_equipment(&mut q);
    let mut y = table_bottom + SECTION_GAP * k;
    y = attunement(cx, (x, y, w));
    y = coins(cx, (x, y, w));
    y = carry(cx, (x, y, w));
    let notes_h = HEAD_H * k + 2.0 * PITCH * k + BLOCK_GAP;
    let rest = bottom - y - notes_h;
    let per = (HEAD_H + BLOCK_GAP) * k;
    let t_lines = (((rest * 0.55 - per) / (PITCH * k)).floor() as i64).max(3) as usize;
    let mut tail: VecDeque<Piece> = VecDeque::new();
    tail.push_back(region(cx, 0, 2, false));
    tail.push_back(region(cx, 1, t_lines, false));
    tail.push_back(region(cx, 2, 2, true));
    fill_last(cx, &mut tail, (x, y, w), bottom, marks);
    q.extend(tail);
    q
}

/// The 2014 inventory page right column: attunement, coins, carrying, then equipment notes.
pub(crate) fn side_2014(
    cx: &mut Cx,
    (x, y0, w): (f32, f32, f32),
    bottom: f32,
    marks: &mut Vec<ContMark>,
) -> VecDeque<Piece> {
    let mut y = attunement(cx, (x, y0, w));
    y = coins(cx, (x, y, w));
    y = carry(cx, (x, y, w));
    let mut q: VecDeque<Piece> = VecDeque::new();
    q.push_back(region(cx, 0, 4, true));
    fill_last(cx, &mut q, (x, y, w), bottom, marks);
    q
}
