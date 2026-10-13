//! Main-flow sections of page 1: weapons, senses and defences, training and proficiencies,
//! conditions with exhaustion, limited uses, features summary and notes.

use super::grid::{wrap_lines, Align, Canvas, Grid, Txt};
use crate::fonts::{text_width, Style};
use crate::model_ids as id;
use crate::model_types::{Edition, EditionFields, SheetData};
use crate::pdf_form::{Mark, Shape};

/// Attack rows printed on page 1.
pub(crate) const ATTACK_CAPACITY: usize = 6;
/// Limited-use rows printed on page 1.
pub(crate) const USE_CAPACITY: usize = 8;
/// Use boxes drawn per limited use.
pub(crate) const MAX_USE_BOXES: i32 = 24;
const ATTACK_ROW_H: f32 = 15.0;
const MIN_ATTACK_ROWS: usize = 5;
const SECTION_GAP: f32 = 8.0;
const LABEL_COL: f32 = 58.0;
const FEATURE_LINES: usize = 4;

fn more(c: &mut Canvas, x_right: f32, base: f32, n: usize, page: u32) -> f32 {
    let t = Txt::new(c.s(6.4), Style::SansItalic, c.pal.ink2).align(Align::End);
    c.put(x_right, base, &t, &format!("+{n} more on page {page}"));
    9.0
}

/// Weapons and damage cantrips. Returns `(y below, overflow)`.
pub(crate) fn weapons(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    d: &SheetData,
    page: u32,
) -> (f32, usize) {
    let is24 = d.edition == Edition::Dnd2024;
    let title = if is24 {
        "Weapons & damage cantrips"
    } else {
        "Attacks & spellcasting"
    };
    let mut y = c.heading(x, y, w, title, None);
    let (bonus_r, dmg_x, props_x) = (x + 134.0, x + 142.0, x + 226.0);
    let hb = y + 6.0;
    c.open_group("Table");
    c.header_row(|c| {
        c.label(x + 2.0, hb, Align::Start, "NAME");
        c.label(bonus_r, hb, Align::End, "HIT / DC");
        c.label(dmg_x, hb, Align::Start, "DAMAGE & TYPE");
        c.label(
            props_x,
            hb,
            Align::Start,
            if is24 {
                "RANGE, PROPERTIES, MASTERY"
            } else {
                "RANGE, NOTES"
            },
        );
    });
    y += 9.5;
    c.hline(x, x + w, y, c.pal.ink, 0.9);
    let pw = x + w - props_x;
    let shown = d.combat.attacks.len().min(ATTACK_CAPACITY);
    for (i, a) in d.combat.attacks.iter().take(shown).enumerate() {
        c.open_group("TR");
        let n = i + 1;
        let top = y;
        let cy = top + ATTACK_ROW_H / 2.0;
        let b = Txt::new(c.s(8.4), Style::SansBold, c.pal.ink);
        c.text_field(
            &id::atk(n, "name"),
            "Attack name",
            (x + 1.0, top + 1.0, 100.0, ATTACK_ROW_H - 2.0),
            cy + 3.0,
            &a.name,
            &b,
        );
        c.text_field(
            &id::atk(n, "bonus"),
            "Attack bonus or save DC",
            (bonus_r - 36.0, top + 1.0, 36.0, ATTACK_ROW_H - 2.0),
            cy + 3.0,
            &a.bonus,
            &b.align(Align::End),
        );
        let dt = Txt::new(c.s(8.0), Style::Sans, c.pal.ink);
        c.text_field(
            &id::atk(n, "damage"),
            "Damage",
            (dmg_x, top + 1.0, 80.0, ATTACK_ROW_H - 2.0),
            cy + 3.0,
            &a.damage,
            &dt,
        );
        let pt = Txt::new(c.s(6.0), Style::Sans, c.pal.ink2);
        let two = !a.notes.is_empty();
        let (b1, b2) = if two {
            (top + 6.2, top + 12.6)
        } else {
            (top + 9.0, top + 12.6)
        };
        let mastery = a.mastery.clone().unwrap_or_default();
        let mut rw = pw;
        if is24 {
            let mw = 30.0;
            if !mastery.is_empty() {
                c.put(
                    props_x + pw - mw - 2.0,
                    b1,
                    &pt.align(Align::End),
                    "mastery",
                );
            }
            c.text_field(
                &id::atk(n, "mastery"),
                "Weapon mastery",
                (props_x + pw - mw, top + b1 - top - 6.0, mw, 8.0),
                b1,
                &mastery,
                &pt.align(Align::End).colored(c.pal.ink),
            );
            rw = pw - mw - 28.0;
        }
        c.text_field(
            &id::atk(n, "range"),
            "Range",
            (props_x, b1 - 6.0, rw, 8.0),
            b1,
            &a.range,
            &pt,
        );
        c.text_field(
            &id::atk(n, "notes"),
            "Attack notes",
            (props_x, b2 - 6.0, pw, 8.0),
            b2,
            &a.notes,
            &pt,
        );
        y += ATTACK_ROW_H;
        c.hline(x, x + w, y, c.pal.rule, 0.5);
        c.close_group();
    }
    for _ in shown..MIN_ATTACK_ROWS {
        y += ATTACK_ROW_H;
        c.hline(x, x + w, y, c.pal.rule, 0.5);
    }
    c.close_group();
    let over = d.combat.attacks.len() - shown;
    if over > 0 {
        y += more(c, x + w, y + 7.5, over, page) - 1.0;
    }
    if is24 && !mastery_list(d).is_empty() {
        y = mastery_chips(c, (x, y + 3.0, w), d);
    }
    (y + SECTION_GAP, over)
}

fn mastery_list(d: &SheetData) -> &[String] {
    match &d.edition_fields {
        EditionFields::Y2024(f) => &f.weapon_masteries,
        EditionFields::Y2014(_) => &[],
    }
}

fn mastery_chips(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    c.label(x + 2.0, y + 8.0, Align::Start, "MASTERY");
    let mut cx = x + LABEL_COL;
    let mut row = y;
    let t = Txt::new(c.s(6.8), Style::SansBold, c.pal.ink);
    for (i, m) in mastery_list(d).iter().enumerate() {
        let cw = text_width(t.style, m, t.size) + 9.0;
        if cx + cw > x + w {
            cx = x + LABEL_COL;
            row += 11.5;
        }
        c.text_field(
            &id::mastery(i + 1),
            "Weapon mastery",
            (cx, row, cw, 10.0),
            row + 7.2,
            m,
            &t.align(Align::Center),
        );
        cx += cw + 4.0;
    }
    row + 12.0
}

fn defenses(d: &SheetData) -> String {
    let x = &d.extras;
    let mut parts: Vec<String> = Vec::new();
    for (label, list) in [
        ("Resist", &x.resistances),
        ("Immune", &x.immunities),
        ("Vulnerable", &x.vulnerabilities),
        ("Condition immune", &x.condition_immunities),
    ] {
        if !list.is_empty() {
            parts.push(format!("{label} {}", list.join(", ")));
        }
    }
    parts.extend(x.advantages.iter().cloned());
    parts.join(" \u{b7} ")
}

fn labelled(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    label: &str,
    (fid, value): (&str, &str),
    lines: usize,
) -> f32 {
    let lt = Txt::new(c.s(6.4), Style::SansBold, c.pal.ink2);
    c.put(x, y + 7.0, &lt.within(LABEL_COL - 4.0), label);
    let vt = Txt::new(c.s(7.4), Style::Sans, c.pal.ink);
    let h = c.multiline_field(
        fid,
        label,
        (x + LABEL_COL, y, w - LABEL_COL),
        value,
        &vt,
        lines,
    );
    let h = h.max(10.0) + 2.0;
    c.hline(x + LABEL_COL, x + w, y + h - 0.5, c.pal.rule, 0.4);
    h
}

/// Senses, defences and extra passive scores.
pub(crate) fn senses(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    let mut y = c.heading(x, y, w, "Senses & defenses", None);
    let senses = d.extras.senses.join(" \u{b7} ");
    y += labelled(c, (x, y, w), "Senses", (id::SENSES, &senses), 2);
    y += labelled(c, (x, y, w), "Defenses", (id::DEFENSES, &defenses(d)), 3);
    for p in d.passives.iter().filter(|p| p.key != "perception") {
        let v = p.value.to_string();
        y += labelled(c, (x, y, w), &p.label, (&id::passive(&p.key), &v), 1);
    }
    y + SECTION_GAP - 2.0
}

fn join(list: &[String]) -> String {
    list.join(", ")
}

/// Training and proficiencies (2024) / proficiencies and languages (2014).
pub(crate) fn training(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    let p = &d.proficiencies;
    if let EditionFields::Y2024(f) = &d.edition_fields {
        let mut y = c.heading(x, y, w, "Training & proficiencies", None);
        let lt = Txt::new(c.s(6.4), Style::SansBold, c.pal.ink2);
        c.put(x, y + 7.0, &lt.within(LABEL_COL - 4.0), "Armor training");
        let at = f.armor_training;
        let ticks = [
            ("Light", at.light),
            ("Medium", at.medium),
            ("Heavy", at.heavy),
            ("Shields", at.shields),
        ];
        let bt = Txt::new(c.s(7.4), Style::Sans, c.pal.ink);
        let mut cx = x + LABEL_COL;
        for (n, (label, on)) in ticks.into_iter().enumerate() {
            c.check(
                id::ARMOR_TRAINING[n],
                label,
                (cx, y + 0.2, 7.4),
                Shape::Square,
                Mark::Tick,
                on,
                true,
            );
            cx += 10.0 + c.put(cx + 10.0, y + 6.6, &bt, label) + 9.0;
        }
        let rest = (x + w - cx).max(20.0);
        let nt = Txt::new(c.s(6.0), Style::SansItalic, c.pal.ink2);
        c.text_field(
            id::PROF_ARMOR,
            "Armor proficiencies",
            (cx, y, rest, 9.5),
            y + 6.6,
            &join(&p.armor),
            &nt,
        );
        y += 12.5;
        y += labelled(
            c,
            (x, y, w),
            "Weapons",
            (id::PROF_WEAPONS, &join(&p.weapons)),
            2,
        );
        y += labelled(c, (x, y, w), "Tools", (id::PROF_TOOLS, &join(&p.tools)), 2);
        y += labelled(
            c,
            (x, y, w),
            "Languages",
            (id::PROF_LANGUAGES, &join(&p.languages)),
            2,
        );
        return y + SECTION_GAP - 2.0;
    }
    let mut y = c.heading(x, y, w, "Proficiencies & languages", None);
    y += labelled(c, (x, y, w), "Armor", (id::PROF_ARMOR, &join(&p.armor)), 2);
    y += labelled(
        c,
        (x, y, w),
        "Weapons",
        (id::PROF_WEAPONS, &join(&p.weapons)),
        2,
    );
    y += labelled(c, (x, y, w), "Tools", (id::PROF_TOOLS, &join(&p.tools)), 2);
    y += labelled(
        c,
        (x, y, w),
        "Languages",
        (id::PROF_LANGUAGES, &join(&p.languages)),
        2,
    );
    y + SECTION_GAP - 2.0
}

/// Conditions (tick boxes) and the exhaustion track.
pub(crate) fn conditions(c: &mut Canvas, (x, y, w): (f32, f32, f32), d: &SheetData) -> f32 {
    let mut y = c.heading(x, y, w, "Conditions", Some("tick while active"));
    let t = Txt::new(c.s(7.2), Style::Sans, c.pal.ink);
    let mut cx = x;
    for name in d.play.conditions {
        let cw = 10.0 + text_width(t.style, name, t.size) + 11.0;
        if cx + cw > x + w + 8.0 {
            cx = x;
            y += c.s(11.4);
        }
        c.check(
            &id::cond(name),
            name,
            (cx, y + 0.3, 7.0),
            Shape::Square,
            Mark::Tick,
            false,
            true,
        );
        c.put(cx + 10.0, y + 6.4, &t, name);
        cx += cw;
    }
    y += c.s(13.0);
    c.label(x, y + 6.0, Align::Start, "EXHAUSTION");
    let ex = d.play.exhaustion;
    for n in 1..=ex.max_levels {
        let bx = x + LABEL_COL + (n - 1) as f32 * 11.0;
        c.check(
            &id::exh(n),
            "Exhaustion level",
            (bx, y, 8.0),
            Shape::Square,
            Mark::Cross,
            n <= ex.level,
            true,
        );
    }
    let hx = x + LABEL_COL + ex.max_levels as f32 * 11.0 + 5.0;
    let hint = if ex.linear {
        "each level: \u{2212}2 to d20 tests and \u{2212}5 ft speed; level 6 is death"
    } else {
        "1 disadvantage on checks, 2 speed halved, 3 disadvantage on attacks and saves, 4 HP max halved, 5 speed 0, 6 death"
    };
    let ht = Txt::new(c.s(5.6), Style::SansItalic, c.pal.ink2);
    for (i, line) in wrap_lines(ht.style, ht.size, x + w - hx, hint)
        .iter()
        .take(3)
        .enumerate()
    {
        c.put(hx, y + 4.4 + i as f32 * 6.8, &ht, line);
    }
    y + 16.0 + SECTION_GAP
}

/// Limited uses with tick boxes. Returns `(y below, overflow)`.
pub(crate) fn limited_uses(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    d: &SheetData,
    page: u32,
) -> (f32, usize) {
    let mut y = c.heading(x, y, w, "Limited uses", Some("refill on the rest shown"));
    let (boxes_x, rech_x) = (x + 158.0, x + w - 64.0);
    c.label(x + 2.0, y + 5.0, Align::Start, "FEATURE");
    c.label(boxes_x, y + 5.0, Align::Start, "USES");
    c.label(rech_x, y + 5.0, Align::Start, "RECHARGE");
    y += 8.0;
    c.hline(x, x + w, y, c.pal.ink, 0.9);
    c.open_group("L");
    let per_row = (((rech_x - 6.0 - boxes_x) / 9.2) as i32).max(4);
    let shown = d.limited_uses.len().min(USE_CAPACITY);
    for (i, u) in d.limited_uses.iter().take(shown).enumerate() {
        c.open_group("LI");
        let n = i + 1;
        let drawn = u.max.min(MAX_USE_BOXES);
        let rows = ((drawn + per_row - 1) / per_row).max(1);
        let rh = 12.5 + (rows - 1) as f32 * 9.0;
        let bt = Txt::new(c.s(8.0), Style::SansBold, c.pal.ink);
        c.text_field(
            &id::use_name(n),
            "Limited use name",
            (x + 1.0, y + 1.0, 152.0, 11.0),
            y + 8.8,
            &u.name,
            &bt,
        );
        for m in 1..=drawn {
            let (col, row) = ((m - 1) % per_row, (m - 1) / per_row);
            let (bx, by) = (boxes_x + col as f32 * 9.2, y + 2.6 + row as f32 * 9.0);
            c.check(
                &id::use_box(n, m),
                "Use",
                (bx, by, 7.0),
                Shape::Circle,
                Mark::Fill,
                false,
                true,
            );
        }
        let rt = Txt::new(c.s(6.8), Style::Sans, c.pal.ink2).within(x + w - rech_x);
        c.put(rech_x, y + 8.6, &rt, &u.recharge);
        y += rh;
        c.hline(x, x + w, y, c.pal.rule, 0.5);
        c.close_group();
    }
    c.close_group();
    let over = d.limited_uses.len() - shown;
    if over > 0 {
        y += more(c, x + w, y + 7.5, over, page);
    }
    (y + SECTION_GAP, over)
}

/// Feature names (2014 summary; the full text lives on the features page).
pub(crate) fn features_summary(
    c: &mut Canvas,
    (x, y, w): (f32, f32, f32),
    d: &SheetData,
    page: u32,
) -> f32 {
    if d.features.is_empty() {
        return y;
    }
    let mut y = c.heading(
        x,
        y,
        w,
        "Features & traits",
        Some("full text on the next pages"),
    );
    let names: Vec<&str> = d.features.iter().map(|f| f.name.as_str()).collect();
    let t = Txt::new(c.s(7.2), Style::Sans, c.pal.ink);
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut used = 0;
    for name in &names {
        let trial = if cur.is_empty() {
            (*name).to_string()
        } else {
            format!("{cur} \u{b7} {name}")
        };
        if text_width(t.style, &trial, t.size) > w {
            if lines.len() + 1 >= FEATURE_LINES {
                break;
            }
            lines.push(std::mem::take(&mut cur));
            cur = (*name).to_string();
        } else {
            cur = trial;
        }
        used += 1;
    }
    lines.push(cur);
    for l in &lines {
        c.put(x, y + 6.0, &t.within(w), l);
        y += c.s(9.4);
    }
    if used < names.len() {
        y += more(c, x + w, y + 5.0, names.len() - used, page);
    }
    y + SECTION_GAP - 2.0
}

/// Notes: a ruled filler down to the page body bottom.
pub(crate) fn notes(c: &mut Canvas, (x, y, w): (f32, f32, f32), g: &Grid) {
    let mut y = c.heading(x, y, w, "Notes", None) + 4.0;
    let pitch = c.s(11.0);
    while y + pitch * 0.5 < g.body_bottom {
        y += pitch;
        c.hline(x, x + w, y, c.pal.rule, 0.5);
    }
}
