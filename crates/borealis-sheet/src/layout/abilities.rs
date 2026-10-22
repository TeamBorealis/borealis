//! Start column of page 1: ability cards with sigils (2024: skills nested; 2014: saves only plus a
//! proficiency block and an alphabetical skill list).

use super::art_place::place;
use super::grid::{Align, Canvas, Col, Grid, Txt};
use super::LayoutOptions;
use crate::art::ArtId;
use crate::fonts::Style;
use crate::model_ids as id;
use crate::model_types::{AbilityData, Edition, SheetData, SkillData};
use crate::pdf_draw::{signed, Paint, Path};
use crate::pdf_form::{Mark, Shape};

const HEADER_H: f32 = 35.5;
const CARD_PAD_BOTTOM: f32 = 4.0;
const CARD_GAP: f32 = 5.0;
const SIGIL_SIZE: f32 = 32.0;
const PIP: f32 = 5.6;
const MAX_ROW_2024: f32 = 12.5;
const MIN_ROW: f32 = 9.6;

/// What one row shows.
struct Row<'a> {
    label: &'a str,
    bold: bool,
    hint: Option<&'a str>,
    value_id: String,
    value: i32,
    prof: (String, bool),
    exp: Option<(String, bool)>,
}

fn abil_row_line(c: &mut Canvas, x: f32, top: f32, w: f32, row_h: f32, r: &Row<'_>) {
    let cy = top + row_h / 2.0;
    let size = c.s(7.4);
    let base = cy + 0.357 * size;
    let expert = r.exp.as_ref().is_some_and(|e| e.1);
    let px = x + 10.0 - PIP / 2.0;
    let py = cy - PIP / 2.0;
    let shape = if expert {
        Shape::Diamond
    } else {
        Shape::Circle
    };
    let tip = format!("{} proficiency", r.label);
    c.check(
        &r.prof.0,
        &tip,
        (px, py, PIP + if expert { 1.6 } else { 0.0 }),
        shape,
        Mark::Fill,
        r.prof.1,
        !expert,
    );
    if let Some((eid, on)) = &r.exp {
        let tip = format!("{} expertise", r.label);
        c.check(
            eid,
            &tip,
            (px, py, PIP + 1.6),
            Shape::Diamond,
            Mark::Fill,
            *on,
            false,
        );
    }
    let style = if r.bold { Style::SansBold } else { Style::Sans };
    let t = Txt::new(size, style, c.pal.ink);
    let nw = c.put(x + 22.0, base, &t.within(w - 22.0 - 40.0), r.label);
    if let Some(h) = r.hint {
        let ht = Txt::new(c.s(5.6), Style::Sans, c.pal.ink2);
        c.put(x + 22.0 + nw + 3.0, base, &ht, h);
    }
    let vr = (x + w - 8.0 - 28.0, top, 28.0, row_h);
    let vt = Txt::new(size, Style::SansBold, c.pal.ink).align(Align::End);
    c.text_field(
        &r.value_id,
        r.label,
        vr,
        base,
        &signed(i64::from(r.value)),
        &vt,
    );
}

fn sigil_id(key: &str) -> ArtId {
    match key {
        "str" => ArtId::SigilStr,
        "dex" => ArtId::SigilDex,
        "con" => ArtId::SigilCon,
        "int" => ArtId::SigilInt,
        "wis" => ArtId::SigilWis,
        _ => ArtId::SigilCha,
    }
}

/// Field names of an ability card's two boxes.
pub(crate) struct CardIds {
    pub score: String,
    pub modifier: String,
}

impl CardIds {
    /// The main sheet's `abil.<key>.score` / `.mod`.
    fn sheet(key: &str) -> CardIds {
        CardIds {
            score: id::abil_score(key),
            modifier: id::abil_mod(key),
        }
    }
}

/// Height of an ability card with no rows below its header.
pub(crate) const HEADER_CARD_H: f32 = HEADER_H + CARD_PAD_BOTTOM;

fn card_header(c: &mut Canvas, rect: (f32, f32, f32), a: &AbilityData, swap: bool, ink: bool) {
    header_with_ids(c, rect, a, (swap, ink), &CardIds::sheet(a.key));
}

/// The ability card header (sigil, name, big box and score ring), with its own field names.
pub(crate) fn header_with_ids(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    a: &AbilityData,
    (swap, ink): (bool, bool),
    ids: &CardIds,
) {
    place(
        c,
        ink,
        sigil_id(a.key),
        (x + 7.0, y + 1.8),
        SIGIL_SIZE / 100.0,
    );
    let nt = Txt::new(c.s(10.6), Style::SerifBold, c.pal.ink);
    c.put(x + 45.0, y + 16.0, &nt.within(66.0), &a.name);
    c.label(x + 45.0, y + 26.0, Align::Start, &a.key.to_uppercase());
    let (big_id, big, small_id, small) = if swap {
        (
            ids.score.clone(),
            a.score.to_string(),
            ids.modifier.clone(),
            signed(i64::from(a.modifier)),
        )
    } else {
        (
            ids.modifier.clone(),
            signed(i64::from(a.modifier)),
            ids.score.clone(),
            a.score.to_string(),
        )
    };
    let bt = Txt::new(c.s(21.0), Style::SerifBold, c.pal.ink).align(Align::End);
    let br = (x + w - 85.0, y + 3.0, 48.0, 29.0);
    c.text_field(&big_id, &a.name, br, y + 24.0, &big, &bt);
    let (cx, cy) = (x + w - 16.5, y + 18.5);
    let ring = Paint::fill(c.pal.paper).with_stroke(c.pal.gold, 0.9);
    c.path(Path::circle(cx, cy, 10.6), ring);
    let st = Txt::new(c.s(9.2), Style::SerifBold, c.pal.ink).align(Align::Center);
    c.text_field(
        &small_id,
        &a.name,
        (cx - 9.0, cy - 7.0, 18.0, 14.0),
        cy + 3.3,
        &small,
        &st,
    );
    c.hline(x + 6.0, x + w - 6.0, y + HEADER_H - 2.0, c.pal.ink, 0.6);
}

fn save_row<'a>(a: &'a AbilityData) -> Row<'a> {
    Row {
        label: "Saving throw",
        bold: true,
        hint: None,
        value_id: id::save(a.key),
        value: a.save,
        prof: (id::save_prof(a.key), a.save_proficient),
        exp: None,
    }
}

fn skill_row<'a>(s: &'a SkillData, hint: Option<&'a str>) -> Row<'a> {
    Row {
        label: &s.name,
        bold: false,
        hint,
        value_id: id::skill(&s.key),
        value: s.total,
        prof: (id::skill_prof(&s.key), s.proficient || s.expertise),
        exp: Some((id::skill_exp(&s.key), s.expertise)),
    }
}

fn legend(c: &mut Canvas, x: f32, y: f32) {
    let t = Txt::new(c.s(6.0), Style::Sans, c.pal.ink2);
    let g = c.pal.gold;
    c.path(Path::circle(x + 2.5, y - 2.0, 2.4), Paint::fill(g));
    let w = c.put(x + 7.0, y, &t, "proficient");
    let dx = x + 7.0 + w + 8.0;
    c.path(Path::diamond(dx + 2.5, y - 2.0, 3.0, 3.0), Paint::fill(g));
    c.put(dx + 8.0, y, &t, "expertise");
}

fn abbr(key: &str) -> String {
    let mut s = key.to_string();
    if let Some(f) = s.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    format!("({s})")
}

/// One list item (`LI`) of a skills / saves list.
fn list_item(c: &mut Canvas, f: impl FnOnce(&mut Canvas)) {
    c.group("LI", f);
}

/// Draw the start column; returns the y below it.
pub(crate) fn column(c: &mut Canvas, g: &Grid, d: &SheetData, o: &LayoutOptions) -> f32 {
    let (x, w) = g.column(Col::Start);
    let nested = d.edition == Edition::Dnd2024;
    let mut y = g.body_top;
    if nested {
        let avail = g.body_bottom - y - 14.0 - 5.0 * CARD_GAP;
        let fixed = 6.0 * (HEADER_H + CARD_PAD_BOTTOM);
        let row_h = ((avail - fixed) / 24.0).clamp(c.s(MIN_ROW), c.s(MAX_ROW_2024));
        for a in &d.abilities {
            let skills: Vec<&SkillData> = d.skills.iter().filter(|s| s.ability == a.key).collect();
            let h = HEADER_H + (skills.len() + 1) as f32 * row_h + CARD_PAD_BOTTOM;
            c.card((x, y, w, h), 5.0, Some(c.pal.paper));
            card_header(c, (x, y, w), a, o.swap_ability_positions, o.ink);
            let mut top = y + HEADER_H;
            c.open_group("L");
            list_item(c, |c| abil_row_line(c, x, top, w, row_h, &save_row(a)));
            for s in skills {
                top += row_h;
                list_item(c, |c| {
                    abil_row_line(c, x, top, w, row_h, &skill_row(s, None))
                });
            }
            c.close_group();
            y += h + CARD_GAP;
        }
        legend(c, x + 2.0, y + 6.0);
        return y + 8.0;
    }
    let row_h = c.s(11.5);
    for a in &d.abilities {
        let h = HEADER_H + row_h + CARD_PAD_BOTTOM;
        c.card((x, y, w, h), 5.0, Some(c.pal.paper));
        card_header(c, (x, y, w), a, o.swap_ability_positions, o.ink);
        abil_row_line(c, x, y + HEADER_H, w, row_h, &save_row(a));
        y += h + 4.0;
    }
    y = core_block_2014(c, (x, y, w), d);
    skills_2014(c, g, (x, y, w), d)
}

/// Inspiration, proficiency bonus and passive Perception (2014).
fn core_block_2014(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    let rh = c.s(16.0);
    let rows = 3.0;
    c.card((x, y, w, rh * rows + 4.0), 5.0, Some(c.pal.paper));
    let passive = d.passives.iter().find(|p| p.key == "perception");
    let mut top = y + 2.0;
    for n in 0..3 {
        let base = top + rh / 2.0 + 2.4;
        let (label, tip) = [
            ("INSPIRATION", "Inspiration"),
            ("PROFICIENCY BONUS", "Proficiency bonus"),
            ("PASSIVE WISDOM (PERCEPTION)", "Passive Wisdom (Perception)"),
        ][n];
        c.label(x + 9.0, base - 0.3, Align::Start, label);
        let vt = Txt::new(c.s(10.0), Style::SerifBold, c.pal.ink).align(Align::End);
        let vr = (x + w - 40.0, top + 1.0, 32.0, rh - 2.0);
        let vbase = top + rh / 2.0 + 3.5;
        match n {
            0 => c.check(
                id::INSPIRATION,
                tip,
                (x + w - 8.0 - 11.0, top + 2.5, 11.0),
                Shape::Diamond,
                Mark::Fill,
                d.play.inspiration,
                true,
            ),
            1 => c.text_field(
                id::PROF_BONUS,
                tip,
                vr,
                vbase,
                &signed(i64::from(d.proficiency_bonus)),
                &vt,
            ),
            _ => {
                let v = passive.map_or(String::new(), |p| p.value.to_string());
                c.text_field(&id::passive("perception"), tip, vr, vbase, &v, &vt);
            }
        }
        if n < 2 {
            c.hline(x + 8.0, x + w - 8.0, top + rh, c.pal.rule, 0.5);
        }
        top += rh;
    }
    y + rh * rows + 4.0 + 5.0
}

/// Alphabetical skill list with the ability abbreviation (2014).
fn skills_2014(c: &mut Canvas, g: &Grid, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    let mut skills: Vec<&SkillData> = d.skills.iter().collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    let n = skills.len().max(1) as f32;
    let avail = g.body_bottom - y - 8.0 - 12.0;
    let row_h = (avail / n).clamp(c.s(8.8), c.s(11.6));
    let h = 12.0 + n * row_h + 4.0;
    c.card((x, y, w, h), 5.0, Some(c.pal.paper));
    c.label(x + 9.0, y + 9.0, Align::Start, "SKILLS");
    let mut top = y + 12.0;
    c.open_group("L");
    for s in skills {
        let hint = abbr(s.ability);
        list_item(c, |c| {
            abil_row_line(c, x, top, w, row_h, &skill_row(s, Some(&hint)))
        });
        top += row_h;
    }
    c.close_group();
    legend(c, x + 2.0, y + h + 8.0);
    y + h + 10.0
}
