//! Spellcasting page parts: the spell table (legend, column header, level headers, rows).

use super::super::grid::{Align, Canvas, Rect, Txt};
use crate::fonts::Style;
use crate::model_ids as id;
use crate::model_types::{CasterData, SpellFacts, SpellRow};
use crate::pdf::PageOp;
use crate::pdf_draw::{Paint, Path};
use crate::pdf_form::{Mark, Shape};

/// Row height.
pub(super) const ROW_H: f32 = 13.0;
/// Level header height.
pub(super) const LEVEL_H: f32 = 12.5;
const LEGEND_H: f32 = 11.0;
const COLHEAD_H: f32 = 11.0;
const HEADING_H: f32 = 17.5;
/// Fixed column widths: pip, level, name, time, range, markers, source (effect takes the rest).
const W_PIP: f32 = 14.0;
const W_LV: f32 = 13.0;
const W_NAME: f32 = 118.0;
const W_TIME: f32 = 52.0;
const W_RANGE: f32 = 46.0;
const W_CRM: f32 = 30.0;
const W_SOURCE: f32 = 50.0;
const PIP: f32 = 7.0;

/// Column x offsets and widths: `(x, w)` per column.
struct Cols {
    pip: (f32, f32),
    lv: (f32, f32),
    name: (f32, f32),
    time: (f32, f32),
    range: (f32, f32),
    crm: (f32, f32),
    effect: (f32, f32),
    source: (f32, f32),
}

fn cols(x: f32, w: f32) -> Cols {
    let fixed = W_PIP + W_LV + W_NAME + W_TIME + W_RANGE + W_CRM + W_SOURCE;
    let mut at = x;
    let mut next = |cw: f32| {
        let col = (at, cw);
        at += cw;
        col
    };
    Cols {
        pip: next(W_PIP),
        lv: next(W_LV),
        name: next(W_NAME),
        time: next(W_TIME),
        range: next(W_RANGE),
        crm: next(W_CRM),
        effect: next(w - fixed),
        source: next(W_SOURCE),
    }
}

/// Height of the table heading, legend and column header.
pub(super) fn intro_height(k: f32) -> f32 {
    (HEADING_H + LEGEND_H + COLHEAD_H) * k
}

/// "1st", "2nd", "3rd", "4th"...
pub(super) fn ordinal(n: u32) -> String {
    let suffix = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

pub(in crate::layout) fn pip_shape(r: &SpellRow) -> (Shape, bool) {
    // Cantrips are always available: the filled diamond, never the hollow "not prepared" circle.
    if r.always_prepared || r.level == 0 {
        (Shape::Diamond, true)
    } else {
        (Shape::Circle, r.prepared)
    }
}

/// Heading, legend and column header of a caster's table; returns the height used.
pub(super) fn intro(c: &mut Canvas, (x, y, w): (f32, f32, f32)) -> f32 {
    let mut top = c.heading(x, y, w, "Cantrips & spells", None);
    legend(c, (x, top, w));
    top += c.s(LEGEND_H);
    top += col_header(c, (x, top, w));
    top - y
}

fn legend(c: &mut Canvas, (x, y, w): (f32, f32, f32)) {
    let t = Txt::new(c.s(6.4), Style::Sans, c.pal.ink2);
    let base = y + c.s(6.5);
    let cy = base - 2.2;
    let mut cx = x + 4.0;
    let gold = Paint::fill(c.pal.gold);
    c.path(Path::circle(cx, cy, 2.9), gold);
    cx += 6.0 + c.put(cx + 6.0, base, &t, "prepared") + 8.0;
    c.path(Path::diamond(cx, cy, 3.3, 3.3), gold);
    cx += 6.0 + c.put(cx + 6.0, base, &t, "always prepared or cantrip") + 8.0;
    c.path(Path::circle(cx, cy, 2.6), Paint::stroke(c.pal.ink, 0.7));
    c.put(cx + 6.0, base, &t, "known or in spellbook, not prepared");
    let key = Txt::new(c.s(6.4), Style::Sans, c.pal.ink2).align(Align::End);
    c.put(
        x + w,
        base,
        &key,
        "C concentration \u{b7} R ritual \u{b7} M material",
    );
}

fn col_header(c: &mut Canvas, (x, y, w): (f32, f32, f32)) -> f32 {
    let k = cols(x, w);
    let base = y + c.s(7.0);
    c.open_table();
    c.header_row(|c| {
        c.label(k.lv.0 + k.lv.1 / 2.0, base, Align::Center, "LV");
        for (col, label) in [
            (k.name, "SPELL"),
            (k.time, "TIME"),
            (k.range, "RANGE"),
            (k.effect, "EFFECT"),
            (k.source, "SOURCE"),
        ] {
            c.label(col.0 + 1.0, base, Align::Start, label);
        }
        c.label(k.crm.0 + k.crm.1 / 2.0, base, Align::Center, "C R M");
    });
    let line = y + c.s(COLHEAD_H) - 1.0;
    c.hline(x, x + w, line, c.pal.ink, 0.9);
    c.s(COLHEAD_H)
}

/// Header of a continuation page's table: the class and the repeated column header.
pub(super) fn continued_head(c: &mut Canvas, (x, y, w): (f32, f32, f32), class: &str) -> f32 {
    let t = Txt::new(c.s(7.6), Style::SerifBold, c.pal.ink);
    let sub = Txt::new(c.s(6.6), Style::SansItalic, c.pal.ink2);
    let base = y + c.s(8.0);
    let used = c.put(x + 1.0, base, &t, class);
    c.put(x + 6.0 + used, base, &sub, "spells, continued");
    let top = y + c.s(11.0);
    top - y + col_header(c, (x, top, w))
}

/// Placeholder line for a caster without listed spells.
pub(super) fn empty(c: &mut Canvas, (x, y, w): (f32, f32, f32)) -> f32 {
    let t = Txt::new(c.s(7.0), Style::SansItalic, c.pal.ink2).within(w);
    c.put(
        x + 4.0,
        y + 10.0,
        &t,
        "No spells chosen yet. Write them in below.",
    );
    for i in 1..=3 {
        c.hline(x, x + w, y + 14.0 + i as f32 * ROW_H, c.pal.rule, 0.5);
    }
    14.0 + 3.0 * ROW_H
}

/// Level header band. `slots` shows "N slots" unless this is a continuation.
pub(super) fn level_header(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    level: u32,
    cont: bool,
    slots: Option<[i32; 9]>,
) -> f32 {
    c.push(PageOp::Rect {
        x,
        y,
        w,
        h: LEVEL_H,
        fill: Some(c.pal.tint),
        stroke: None,
        width: 0.0,
    });
    let title = if level == 0 {
        "Cantrips".to_string()
    } else {
        format!("{} level", ordinal(level))
    };
    let main = Txt::new(c.s(7.4), Style::SerifBold, c.pal.ink);
    let base = y + 8.9;
    let mut at = x + 4.0 + c.put(x + 4.0, base, &main, &title);
    let sub = Txt::new(c.s(6.6), Style::Sans, c.pal.ink2);
    if cont {
        c.put(at + 3.0, base, &sub, "(cont.)");
        return LEVEL_H;
    }
    let count = slots.and_then(|s| {
        usize::try_from(level)
            .ok()
            .and_then(|l| l.checked_sub(1))
            .map(|i| s[i])
    });
    if let Some(n) = count.filter(|n| *n > 0 && level > 0) {
        let unit = if n == 1 { "slot" } else { "slots" };
        at += 3.0;
        c.put(at, base, &sub, &format!("\u{b7} {n} {unit}"));
    }
    LEVEL_H
}

fn has_material(f: &SpellFacts) -> bool {
    let head = f.components.as_deref().unwrap_or("");
    let head = head.split('(').next().unwrap_or("");
    head.split(',').any(|t| t.trim() == "M")
}

/// The C / R / M marker string of a spell.
pub(super) fn markers(f: Option<&SpellFacts>) -> String {
    let Some(f) = f else { return String::new() };
    let mut m: Vec<&str> = Vec::new();
    if f.concentration {
        m.push("C");
    }
    if f.ritual {
        m.push("R");
    }
    if has_material(f) {
        m.push("M");
    }
    m.join(" ")
}

/// Source label of a row.
fn source(row: &SpellRow, caster: &CasterData) -> String {
    if let Some(s) = row
        .source
        .as_ref()
        .filter(|s| !s.eq_ignore_ascii_case("spellcasting"))
    {
        return s.clone();
    }
    let listed = row.prepared || row.always_prepared || !caster.prepares;
    if listed || row.level == 0 {
        caster.class.clone()
    } else if row.known {
        "Spellbook".to_string()
    } else {
        "Class list".to_string()
    }
}

/// One spell row. `ids` is `(caster number, row index)`; returns the row height.
pub(super) fn row(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    ids: (usize, usize),
    (row, caster): (&SpellRow, &CasterData),
    sfx: &str,
) -> f32 {
    let (n, r) = ids;
    let k = cols(x, w);
    let fid = |part: &str| format!("{}{sfx}", id::spell_row(n, r, part));
    let (shape, checked) = pip_shape(row);
    let ps = (k.pip.0 + (W_PIP - PIP) / 2.0, y + (ROW_H - PIP) / 2.0, PIP);
    c.check(
        &fid("prep"),
        "Prepared",
        ps,
        shape,
        Mark::Fill,
        checked,
        !checked,
    );
    let facts = row.facts.as_ref();
    let get = |f: fn(&SpellFacts) -> &Option<String>| -> String {
        facts.and_then(|f2| f(f2).clone()).unwrap_or_default()
    };
    let base = y + 9.3;
    let fr = |col: (f32, f32)| -> Rect { (col.0, y + 1.0, col.1 - 2.0, ROW_H - 2.0) };
    let name_style = if row.prepared || row.always_prepared || row.level == 0 {
        Style::SansBold
    } else {
        Style::Sans
    };
    let size = c.s(7.6);
    let small = c.s(6.6);
    let ink = c.pal.ink;
    let lv = Txt::new(small, Style::Sans, c.pal.ink2).align(Align::Center);
    c.text_field(
        &fid("level"),
        "Spell level",
        fr(k.lv),
        base,
        &row.level.to_string(),
        &lv,
    );
    c.text_field(
        &fid("name"),
        "Spell name",
        fr(k.name),
        base,
        &row.name,
        &Txt::new(size, name_style, ink),
    );
    let plain = Txt::new(c.s(7.0), Style::Sans, ink);
    let time = get(|f| &f.casting_time);
    c.text_field(
        &fid("time"),
        "Casting time",
        fr(k.time),
        base,
        &time,
        &plain,
    );
    let range = get(|f| &f.range);
    c.text_field(&fid("range"), "Range", fr(k.range), base, &range, &plain);
    let mark = Txt::new(c.s(7.0), Style::SansBold, ink).align(Align::Center);
    c.text_field(
        &fid("comp"),
        "Concentration, ritual, material",
        fr(k.crm),
        base,
        &markers(facts),
        &mark,
    );
    let effect = get(|f| &f.effect);
    c.text_field(
        &fid("effect"),
        "Effect",
        fr(k.effect),
        base,
        &effect,
        &Txt::new(c.s(6.8), Style::Sans, ink),
    );
    let st = Txt::new(c.s(6.2), Style::Sans, c.pal.ink2).within(k.source.1 - 2.0);
    c.put(k.source.0, base, &st, &source(row, caster));
    c.hline(x, x + w, y + ROW_H, c.pal.rule, 0.5);
    ROW_H
}
