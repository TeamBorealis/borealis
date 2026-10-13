//! Vitals (AC shield, hit points, hit dice, death saves) and the stat strip.

use super::grid::{wrap_lines, Align, Canvas, Txt};
use crate::fonts::Style;
use crate::model_ids as id;
use crate::model_types::{Edition, SheetData};
use crate::pdf_draw::{signed, Paint, Path, Seg};
use crate::pdf_form::{Mark, Shape};

const AC_W: f32 = 60.0;
const HD_W: f32 = 94.0;
const BOX_GAP: f32 = 7.0;
const MIN_H: f32 = 65.0;
/// Noto cap height, in em.
const CAP_HEIGHT: f32 = 0.714;
const DICE_PER_ROW: u32 = 8;
const STRIP_H: f32 = 30.0;
const STRIP_GAP: f32 = 4.0;

fn shield(x: f32, y: f32, w: f32, h: f32) -> Path {
    let r = 6.0;
    Path::new(vec![
        Seg::Move(x + r, y),
        Seg::Line(x + w - r, y),
        Seg::Cubic(x + w - 2.7, y, x + w, y + 2.7, x + w, y + r),
        Seg::Line(x + w, y + h * 0.6),
        Seg::Cubic(
            x + w,
            y + h * 0.84,
            x + w * 0.62,
            y + h - 3.0,
            x + w / 2.0,
            y + h,
        ),
        Seg::Cubic(x + w * 0.38, y + h - 3.0, x, y + h * 0.84, x, y + h * 0.6),
        Seg::Line(x, y + r),
        Seg::Cubic(x, y + 2.7, x + 2.7, y, x + r, y),
        Seg::Close,
    ])
}

fn hd_rows(d: &SheetData) -> u32 {
    d.combat
        .hit_dice
        .iter()
        .map(|h| h.count.div_ceil(DICE_PER_ROW))
        .sum()
}

/// Draws the vitals row; returns its height.
pub(crate) fn vitals(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    let h = MIN_H.max(14.0 + hd_rows(d) as f32 * 9.6 + 36.0);
    let ink = c.pal.ink;
    // Armor class shield.
    c.path(
        shield(x, y, AC_W, h),
        Paint::fill(c.pal.paper).with_stroke(ink, 0.9),
    );
    c.label(x + AC_W / 2.0, y + 12.0, Align::Center, "ARMOR CLASS");
    let at = Txt::new(c.s(23.0), Style::SerifBold, ink).align(Align::Center);
    let ac = d.combat.armor_class.to_string();
    c.text_field(
        id::AC,
        "Armor class",
        (x + 8.0, y + 14.0, AC_W - 16.0, 26.0),
        y + 33.0,
        &ac,
        &at,
    );
    let nt = Txt::new(c.s(5.8), Style::Sans, c.pal.ink2).align(Align::Center);
    let note = d.combat.ac_note.as_str();
    let lines = wrap_lines(nt.style, nt.size, AC_W - 14.0, note)
        .len()
        .max(1);
    let nh = nt.size * 1.3 * lines.min(2) as f32 + 1.0;
    c.multiline_field(
        id::AC_NOTE,
        "Armor class note",
        (x + 6.0, y + 37.0, AC_W - 12.0),
        note,
        &nt,
        2,
    );
    if d.edition == Edition::Dnd2024 {
        let ty = y + (37.0 + nh + 3.0).max(h - 17.0).min(h - 14.0);
        c.check(
            id::AC_SHIELD,
            "Shield equipped",
            (x + 14.0, ty, 7.0),
            Shape::Square,
            Mark::Tick,
            d.combat.shield_equipped,
            true,
        );
        c.put(
            x + 24.0,
            ty + 6.0,
            &Txt::new(c.s(6.2), Style::Sans, c.pal.ink2),
            "shield",
        );
    }
    // Hit points.
    let hx = x + AC_W + BOX_GAP;
    let hw = w - AC_W - HD_W - 2.0 * BOX_GAP;
    c.card((hx, y, hw, h), 5.0, Some(c.pal.paper));
    c.label(hx + 7.0, y + 12.0, Align::Start, "HP MAX");
    let ht = Txt::new(c.s(23.0), Style::SerifBold, ink);
    c.text_field(
        id::HP_MAX,
        "Hit points, maximum",
        (hx + 5.0, y + 16.0, 42.0, 28.0),
        y + 38.0,
        &d.combat.hp_max.to_string(),
        &ht,
    );
    let bx = hx + 52.0;
    let bw = (hw - 52.0 - 8.0 - 6.0) / 2.0;
    let bh = (h - 22.0).min(38.0);
    c.label(bx, y + 12.0, Align::Start, "CURRENT");
    c.write_box(
        id::HP_CURRENT,
        "Hit points, current",
        (bx, y + 16.0, bw, bh),
        13.0,
    );
    c.label(bx + bw + 6.0, y + 12.0, Align::Start, "TEMPORARY");
    c.write_box(
        id::HP_TEMP,
        "Hit points, temporary",
        (bx + bw + 6.0, y + 16.0, bw, bh),
        13.0,
    );
    // Hit dice and death saves.
    let dx = x + w - HD_W;
    c.card((dx, y, HD_W, h), 5.0, Some(c.pal.paper));
    let total: String = d
        .combat
        .hit_dice
        .iter()
        .map(|h| format!("{}d{}", h.count, h.die))
        .collect::<Vec<_>>()
        .join(" + ");
    c.label(dx + 7.0, y + 11.0, Align::Start, "HIT DICE");
    let tt = Txt::new(c.s(6.2), Style::SansBold, ink).align(Align::End);
    c.text_field(
        id::HD_TOTAL,
        "Hit dice",
        (dx + HD_W - 50.0, y + 4.5, 43.0, 8.5),
        y + 11.0,
        &total,
        &tt,
    );
    let mut ry = y + 15.0;
    for hd in &d.combat.hit_dice {
        let tt = Txt::new(c.s(6.2), Style::SansBold, c.pal.ink2);
        let lt = (dx + 7.0, ry, 21.0, 8.0);
        c.text_field(
            &id::hd_total(hd.die),
            "Hit dice of this size",
            lt,
            ry + 6.2,
            &format!("d{} \u{d7}{}", hd.die, hd.count),
            &tt,
        );
        for n in 0..hd.count {
            let (col, row) = (n % DICE_PER_ROW, n / DICE_PER_ROW);
            let bx = dx + 30.0 + col as f32 * 7.6;
            let by = ry + row as f32 * 9.4 + 0.5;
            c.check(
                &id::hd_spent(hd.die, n + 1),
                "Hit die spent",
                (bx, by, 6.2),
                Shape::Square,
                Mark::Cross,
                false,
                true,
            );
        }
        ry += hd.count.div_ceil(DICE_PER_ROW) as f32 * 9.4 + 1.0;
    }
    let dy = (y + h - 28.0).max(ry + 3.0);
    c.hline(dx + 7.0, dx + HD_W - 7.0, dy - 3.0, c.pal.rule, 0.5);
    c.label(dx + 7.0, dy + 5.0, Align::Start, "DEATH SAVES");
    let lt = Txt::new(c.s(6.4), Style::Sans, c.pal.ink2);
    for (row, (label, ids)) in [("Success", true), ("Failure", false)]
        .into_iter()
        .enumerate()
    {
        let by = dy + 8.0 + row as f32 * 9.4;
        c.put(dx + 7.0, by + 6.0, &lt, label);
        for n in 1..=3u32 {
            let fid = if ids {
                id::death_success(n)
            } else {
                id::death_failure(n)
            };
            let bx = dx + 40.0 + (n - 1) as f32 * 11.0;
            c.check(
                &fid,
                label,
                (bx, by, 7.4),
                Shape::Circle,
                Mark::Fill,
                false,
                true,
            );
        }
    }
    h
}

fn cell(c: &mut Canvas, (x, y, w): (f32, f32, f32), label: &str) {
    c.card((x, y, w, STRIP_H), 4.0, Some(c.pal.paper));
    c.label(x + w / 2.0, y + 8.5, Align::Center, label);
}

/// The stat strip; returns its height.
pub(crate) fn strip(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    let is24 = d.edition == Edition::Dnd2024;
    let n = if is24 { 6.0 } else { 3.0 };
    let cw = (w - (n - 1.0) * STRIP_GAP) / n;
    let vt = Txt::new(c.s(14.0), Style::SerifBold, c.pal.ink).align(Align::Center);
    let mut cells: Vec<(&str, &str, String, &str, Option<String>)> = Vec::new();
    let pb = signed(i64::from(d.proficiency_bonus));
    let walk = d.combat.speeds.first().map_or(0, |s| s.1);
    let others: Vec<String> = d
        .combat
        .speeds
        .iter()
        .skip(1)
        .map(|(k, v)| format!("{} {v}", k.to_lowercase()))
        .collect();
    let adv = d
        .combat
        .initiative_advantage
        .then(|| "advantage".to_string());
    if is24 {
        cells.push(("PROFICIENCY", id::PROF_BONUS, pb, "Proficiency bonus", None));
    }
    cells.push((
        "INITIATIVE",
        id::INITIATIVE,
        signed(i64::from(d.combat.initiative)),
        "Initiative",
        adv,
    ));
    cells.push((
        "SPEED",
        id::SPEED,
        format!("{walk} ft"),
        "Speed",
        (!others.is_empty()).then(|| others.join(" \u{b7} ")),
    ));
    cells.push((
        "SIZE",
        id::CHAR_SIZE,
        d.identity.size.clone().unwrap_or_default(),
        "Size",
        None,
    ));
    let mut cx = x;
    for (label, fid, value, tip, sub) in cells {
        cell(c, (cx, y, cw), label);
        let big = if fid == id::CHAR_SIZE {
            c.s(11.0)
        } else {
            c.s(14.0)
        };
        let t = vt.sized(big);
        // A smaller value (SIZE's word) sits on a raised baseline so its capitals share the
        // optical middle of the 14 pt numerals beside it.
        let lift = (c.s(14.0) - big) * CAP_HEIGHT / 2.0;
        let base = y + if sub.is_some() { 21.0 } else { 23.5 } - lift;
        c.text_field(
            fid,
            tip,
            (cx + 3.0, y + 10.5, cw - 6.0, 16.0),
            base,
            &value,
            &t,
        );
        if let Some(s) = sub {
            let st = Txt::new(c.s(5.2), Style::Sans, c.pal.ink2)
                .align(Align::Center)
                .within(cw - 6.0);
            c.put(cx + cw / 2.0, y + 27.2, &st, &s);
        }
        cx += cw + STRIP_GAP;
    }
    if is24 {
        let p = d.passives.iter().find(|p| p.key == "perception");
        cell(c, (cx, y, cw), "PASSIVE PERC.");
        let v = p.map_or(String::new(), |p| p.value.to_string());
        c.text_field(
            &id::passive("perception"),
            "Passive Perception",
            (cx + 3.0, y + 10.5, cw - 6.0, 16.0),
            y + 23.5,
            &v,
            &vt,
        );
        cx += cw + STRIP_GAP;
        cell(c, (cx, y, cw), "HEROIC INSP.");
        let s = 14.0;
        c.check(
            id::HEROIC_INSPIRATION,
            "Heroic Inspiration",
            (cx + cw / 2.0 - s / 2.0, y + 11.5, s),
            Shape::Diamond,
            Mark::Fill,
            d.play.inspiration,
            true,
        );
    }
    STRIP_H
}
