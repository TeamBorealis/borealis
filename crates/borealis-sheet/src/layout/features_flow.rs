//! The block flow shared by the features, gear and story pages: pieces (headings, feature
//! blocks, ruled write-in regions, equipment rows, the appearance block), their measured heights,
//! splitting between paragraphs, and `fill`, which pours a queue of pieces into one column.

use super::features_text::{line_count, split_text, Ids};
use crate::fonts::{text_width, Style};
use crate::layout::grid::{Align, Canvas, Txt, FIELD_PAD};
use crate::model_ids as id;
use crate::model_types::SheetData;
use crate::pdf::PageOp;
use crate::pdf_draw::{Paint, Path};
use crate::pdf_form::{FieldKind, FormField, TextField};
use std::collections::VecDeque;

pub(crate) const HEAD_H: f32 = 17.5;
pub(crate) const BLOCK_GAP: f32 = 5.0;
pub(crate) const NAME_H: f32 = 11.0;
pub(crate) const CONT_H: f32 = 9.0;
pub(crate) const BODY: f32 = 7.4;
pub(crate) const LEAD_K: f32 = 1.3;
pub(crate) const PITCH: f32 = 11.0;
pub(crate) const ROW_H: f32 = 13.5;
pub(crate) const COLHEAD_H: f32 = 11.0;
pub(crate) const PORTRAIT_W: f32 = 84.0;
pub(crate) const PORTRAIT_H: f32 = 108.0;
const EPS: f32 = 0.01;
/// Space between the last equipment row and a following block.
pub(crate) const AFTER_TABLE: f32 = 6.0;
/// Room kept at a column bottom for the "continued on page N" note.
pub(crate) const MARK_H: f32 = 9.0;
/// Size of the "continued on page N" note.
pub(crate) const MARK_SIZE: f32 = 5.8;
/// Shortest head or tail of a split block, in lines.
const MIN_PART: usize = 2;

/// A feature, trait or feat block (one chunk of it).
#[derive(Debug, Clone)]
pub(crate) struct Feat {
    /// 1-based index in `SheetData::features` (the `feat.N.*` ids).
    pub n: usize,
    pub name: String,
    /// Gold caps tag ("WIZARD 1").
    pub tag: String,
    /// Decorative use pips (the real boxes are on page 1).
    pub pips: i32,
    pub text: String,
    /// A later chunk of a block that was split.
    pub cont: bool,
    /// Full text of a feature that is only summarised on the main page.
    pub detail: bool,
    /// This chunk shows the summary: point at the details page.
    pub summarised: bool,
    /// Heading of the block's group ("Feats"), repeated as "Feats (continued)" when the group
    /// goes on on a new page (empty: none).
    pub group: String,
}

/// A heading plus a ruled write-in area (a multi-line field).
#[derive(Debug, Clone)]
pub(crate) struct Region {
    pub id: String,
    pub tip: String,
    pub title: String,
    pub text: String,
    pub min_lines: usize,
    /// Grows to the column bottom when it is the last piece.
    pub fill: bool,
}

/// Which appearance block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppKind {
    /// 2024 Story page: alignment cell, no deity.
    Story,
    /// 2014 Details page: deity cell.
    Details,
}

/// One flowing piece.
#[derive(Debug, Clone)]
pub(crate) enum Piece {
    Heading { title: String, hint: Option<String> },
    Feature(Feat),
    Region(Region),
    EquipHead { title: String, hint: Option<String> },
    EquipRow(usize),
    Appearance(AppKind),
}

/// Where a "continued on page N" note goes: the block above it goes on on the page after
/// `page` (filled in once page numbers are known).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContMark {
    pub page: usize,
    pub x_right: f32,
    pub base: f32,
}

/// Where "see the details page" notes go (filled in once page numbers are known).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DetailRef {
    pub n: usize,
    pub page: usize,
    pub x_right: f32,
    pub base: f32,
}

/// Everything drawing a piece needs.
pub(crate) struct Cx<'a> {
    pub c: &'a mut Canvas,
    pub ids: &'a mut Ids,
    pub d: &'a SheetData,
    /// Page ordinal within its sequence (1 = first page).
    pub ordinal: u32,
    /// Index of the page in the output (for `DetailRef`).
    pub page_idx: usize,
    pub refs: &'a mut Vec<DetailRef>,
    /// `(feature n, page index)` of every full-text detail block drawn.
    pub landed: &'a mut Vec<(usize, usize)>,
}

pub(crate) fn body_size(k: f32) -> f32 {
    BODY * k
}

fn lead(k: f32) -> f32 {
    body_size(k) * LEAD_K
}

fn text_lines(k: f32, w: f32, text: &str) -> usize {
    line_count(Style::Sans, body_size(k), w - 2.0 * FIELD_PAD, text)
}

fn feat_rows(f: &Feat, k: f32) -> f32 {
    if f.cont {
        CONT_H * k
    } else {
        NAME_H * k
    }
}

fn feat_h(f: &Feat, k: f32, w: f32) -> f32 {
    feat_rows(f, k) + text_lines(k, w, &f.text) as f32 * lead(k) + 1.0 + BLOCK_GAP
}

fn region_lines(r: &Region, k: f32, w: f32) -> usize {
    let n = if r.text.is_empty() {
        0
    } else {
        text_lines(k, w, &r.text)
    };
    n.max(r.min_lines).max(1)
}

fn region_h(r: &Region, k: f32, w: f32) -> f32 {
    (HEAD_H + 0.0) * k + region_lines(r, k, w) as f32 * PITCH * k + BLOCK_GAP
}

fn appearance_h(k: f32) -> f32 {
    HEAD_H * k + PORTRAIT_H + 6.0
}

/// Height of a piece in a column of width `w`.
pub(crate) fn height(p: &Piece, k: f32, w: f32) -> f32 {
    match p {
        Piece::Heading { .. } => HEAD_H * k,
        Piece::Feature(f) => feat_h(f, k, w),
        Piece::Region(r) => region_h(r, k, w),
        Piece::EquipHead { .. } => (HEAD_H + COLHEAD_H) * k,
        Piece::EquipRow(_) => ROW_H * k,
        Piece::Appearance(_) => appearance_h(k),
    }
}

/// Smallest height a piece may take when it is split (its own height when it cannot split).
fn min_h(p: &Piece, k: f32, w: f32) -> f32 {
    match p {
        Piece::Feature(f) if text_lines(k, w, &f.text) > MIN_PART + 1 => {
            feat_rows(f, k) + MIN_PART as f32 * lead(k) + 1.0 + BLOCK_GAP
        }
        Piece::Region(r) if !r.text.is_empty() && text_lines(k, w, &r.text) > MIN_PART + 1 => {
            HEAD_H * k + MIN_PART as f32 * PITCH * k + BLOCK_GAP
        }
        _ => height(p, k, w),
    }
}

/// Split a piece so the head fits `room`; `None` when it cannot be split that way.
fn split(p: &Piece, k: f32, w: f32, room: f32) -> Option<(Piece, Piece)> {
    match p {
        Piece::Feature(f) => {
            let avail = room - feat_rows(f, k) - 1.0 - BLOCK_GAP;
            let max_lines = (avail / lead(k)).floor().max(0.0) as usize;
            if max_lines < MIN_PART {
                return None;
            }
            let (head, tail) = split_text(
                Style::Sans,
                body_size(k),
                w - 2.0 * FIELD_PAD,
                &f.text,
                max_lines,
                MIN_PART,
            )?;
            let a = Feat {
                text: head,
                ..f.clone()
            };
            let b = Feat {
                text: tail,
                cont: true,
                ..f.clone()
            };
            Some((Piece::Feature(a), Piece::Feature(b)))
        }
        Piece::Region(r) if !r.text.is_empty() => {
            let avail = room - HEAD_H * k - BLOCK_GAP;
            let max_lines = (avail / (PITCH * k)).floor().max(0.0) as usize;
            if max_lines < MIN_PART {
                return None;
            }
            let (head, tail) = split_text(
                Style::Sans,
                body_size(k),
                w - 2.0 * FIELD_PAD,
                &r.text,
                max_lines,
                MIN_PART,
            )?;
            let a = Region {
                text: head,
                fill: false,
                ..r.clone()
            };
            let b = Region {
                text: tail,
                title: continued(&r.title),
                ..r.clone()
            };
            Some((Piece::Region(a), Piece::Region(b)))
        }
        _ => None,
    }
}

pub(crate) fn continued(title: &str) -> String {
    if title.ends_with("(continued)") {
        title.to_string()
    } else {
        format!("{title} (continued)")
    }
}

fn tag_for(d: &SheetData, f: &crate::model_types::FeatureData) -> String {
    use crate::model_types::FeatureGroup as G;
    let up = |s: &str| s.to_uppercase();
    match f.group {
        G::Class | G::Subclass => match (&f.class, f.level) {
            (Some(c), l) if l > 0 => format!("{} {l}", up(c)),
            (Some(c), _) => up(c),
            (None, l) if l > 0 => format!("LEVEL {l}"),
            _ => String::new(),
        },
        G::Species => String::new(),
        G::Feat if f.level > 0 => format!("LEVEL {}", f.level),
        G::Background => up(&d.identity.background),
        _ => String::new(),
    }
}

/// Feature block for feature `i` (0-based).
pub(crate) fn feature_piece(d: &SheetData, i: usize, summary: bool) -> Feat {
    let f = &d.features[i];
    let pips = d
        .limited_uses
        .iter()
        .find(|u| u.name == f.name || u.name.starts_with(&f.name))
        .map_or(0, |u| u.max.min(6));
    let text = if summary && !f.summary.is_empty() {
        f.summary.clone()
    } else {
        f.text.clone()
    };
    Feat {
        n: i + 1,
        name: f.name.clone(),
        tag: tag_for(d, f),
        pips,
        text,
        cont: false,
        detail: false,
        summarised: summary && f.summary != f.text,
        group: String::new(),
    }
}

fn draw_feature(cx: &mut Cx, f: &Feat, (x, y, w): (f32, f32, f32)) {
    let k = cx.c.k;
    let (ord, base_name, base_text) = (cx.ordinal, id::feat(f.n, "name"), id::feat(f.n, "text"));
    let ink = cx.c.pal.ink;
    let mut y_text = y + NAME_H * k;
    if f.cont {
        let t = Txt::new(7.0 * k, Style::SansItalic, cx.c.pal.ink2).within(w);
        cx.c.put(x, y + 7.0 * k, &t, &format!("{} (continued)", f.name));
        y_text = y + CONT_H * k;
    } else {
        let t = Txt::new(8.2 * k, Style::SansBold, ink);
        let nw = (text_width(Style::SansBold, &f.name, 8.2 * k) + 3.0).min(w * 0.62);
        if f.detail {
            cx.landed.push((f.n, cx.page_idx));
            cx.c.put(x, y + 8.2 * k, &t.within(nw), &f.name);
        } else {
            let nid = cx.ids.make(&base_name, ord);
            cx.c.text_field(
                &nid,
                "Feature name",
                (x, y, nw, 10.6 * k),
                y + 8.2 * k,
                &f.name,
                &t,
            );
        }
        let mut cur = x + nw + 4.0;
        if !f.tag.is_empty() {
            let tw = cx.c.label_in(
                cur,
                y + 7.8 * k,
                Align::Start,
                &f.tag,
                cx.c.pal.gold,
                5.2 * k,
            );
            cur += tw + 5.0;
        }
        for _ in 0..f.pips {
            let circle = Path::circle(cur + 2.4, y + 5.2 * k, 2.4);
            cx.c.path(circle, Paint::stroke(cx.c.pal.gold, 0.8));
            cur += 6.4;
        }
        if f.summarised {
            cx.refs.push(DetailRef {
                n: f.n,
                page: cx.page_idx,
                x_right: x + w,
                base: y + 7.6 * k,
            });
        }
    }
    let lines = text_lines(k, w, &f.text);
    let tid = cx.ids.make(&base_text, ord);
    let h = lines as f32 * lead(k) + 1.0;
    let mut tf = TextField::new(&crate::pdf_draw::typographic_minus(&f.text), body_size(k));
    tf.style = Style::Sans;
    tf.color = ink;
    tf.multiline = true;
    tf.leading = Some(lead(k));
    tf.pad = FIELD_PAD;
    tf.baseline = Some(crate::pdf::BASELINE * body_size(k) + 0.4);
    let field =
        FormField::new(&tid, x, y_text, w, h, FieldKind::Text(tf)).with_tooltip(if f.detail {
            "Feature text (full)"
        } else {
            "Feature text"
        });
    cx.c.push(PageOp::Field(field));
}

fn draw_region(cx: &mut Cx, r: &Region, (x, y, w): (f32, f32, f32), h: f32) {
    let k = cx.c.k;
    let top = cx.c.heading(x, y, w, &r.title, None);
    let rows = ((h - HEAD_H * k - BLOCK_GAP) / (PITCH * k) + 0.01)
        .floor()
        .max(1.0);
    let area_h = rows * PITCH * k;
    let rid = cx.ids.make(&r.id, cx.ordinal);
    cx.c.ruled_field(
        &rid,
        &r.tip,
        (x, top, w, area_h),
        &r.text,
        (PITCH * k, body_size(k)),
    );
}

fn draw_equip_head(cx: &mut Cx, title: &str, hint: Option<&str>, (x, y, w): (f32, f32, f32)) {
    let k = cx.c.k;
    let top = cx.c.heading(x, y, w, title, hint);
    let lb = top + 5.0 * k;
    cx.c.open_table();
    cx.c.header_row(|c| {
        c.label(x + 1.0, lb, Align::Start, "EQ.");
        c.label(x + 16.0, lb, Align::Start, "ITEM");
        c.label(x + w - 36.0, lb, Align::End, "QTY");
        c.label(x + w - 2.0, lb, Align::End, "LB");
    });
    cx.c.hline(x, x + w, top + 8.5 * k, cx.c.pal.ink, 0.9);
}

/// One equipment row as a `TR` of the open equipment table (a table is opened when the column
/// starts with rows only).
fn draw_equip_row(cx: &mut Cx, i: usize, at: (f32, f32, f32)) {
    cx.c.open_table();
    cx.c.open_group("TR");
    equip_row_cells(cx, i, at);
    cx.c.close_group();
}

fn equip_row_cells(cx: &mut Cx, i: usize, (x, y, w): (f32, f32, f32)) {
    let k = cx.c.k;
    let n = i + 1;
    let item = &cx.d.gear.items[i];
    let h = ROW_H * k;
    let ord = cx.ordinal;
    let (cy, base) = (y + h / 2.0, y + h / 2.0 + 2.8 * k);
    let dot = Path::circle(x + 4.0, cy, 2.4);
    let paint = if item.equipped.is_empty() {
        Paint::stroke(cx.c.pal.rule, 0.7)
    } else {
        Paint::fill(cx.c.pal.gold)
    };
    cx.c.path(dot, paint);
    let t = Txt::new(8.0 * k, Style::Sans, cx.c.pal.ink);
    let nid = cx.ids.make(&id::equip(n, "name"), ord);
    cx.c.text_field(
        &nid,
        "Item name",
        (x + 10.0, y + 1.0, w - 10.0 - 62.0, h - 2.0),
        base,
        &item.name,
        &t,
    );
    let qid = cx.ids.make(&id::equip(n, "qty"), ord);
    cx.c.text_field(
        &qid,
        "Quantity",
        (x + w - 60.0, y + 1.0, 26.0, h - 2.0),
        base,
        &item.quantity.to_string(),
        &t.align(Align::End),
    );
    let wid = cx.ids.make(&id::equip(n, "weight"), ord);
    let weight = if item.weight > 0.0 {
        trim_num(item.weight)
    } else {
        "\u{2014}".to_string()
    };
    cx.c.text_field(
        &wid,
        "Weight in pounds",
        (x + w - 32.0, y + 1.0, 32.0, h - 2.0),
        base,
        &weight,
        &t.align(Align::End),
    );
    cx.c.hline(x, x + w, y + h, cx.c.pal.rule, 0.5);
}

/// `3`, `2.5`, `3,724.56`, never `3.0` (thousands grouped like XP).
pub(crate) fn trim_num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    match s.split_once('.') {
        Some((int, frac)) => format!("{}.{frac}", group_digits(int)),
        None => group_digits(s),
    }
}

/// Digits with a comma every three places (`-1234` -> `-1,234`).
pub(crate) fn group_digits(s: &str) -> String {
    let (sign, digits) = s.strip_prefix('-').map_or(("", s), |d| ("-", d));
    let mut out = String::from(sign);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Blank ruled rows (write-in lines) down to `bottom`.
pub(crate) fn blank_rows(c: &mut Canvas, (x, y, w): (f32, f32, f32), bottom: f32) {
    let h = ROW_H * c.k;
    let mut y = y;
    while y + h <= bottom + EPS {
        y += h;
        c.hline(x, x + w, y, c.pal.rule, 0.5);
    }
}

fn draw_appearance(cx: &mut Cx, kind: AppKind, (x, y, w): (f32, f32, f32)) {
    let k = cx.c.k;
    let top = cx.c.heading(x, y, w, "Appearance", None);
    let (pw, ph) = (PORTRAIT_W, PORTRAIT_H);
    let d = cx.d;
    cx.c.artifact(|c| {
        c.card((x, top, pw, ph), 3.0, None);
        if !d.story.has_portrait {
            let t = Txt::new(5.4 * k, Style::SansBold, c.pal.rule).align(Align::Center);
            c.label_in(
                x + pw / 2.0,
                top + ph / 2.0,
                Align::Center,
                "PORTRAIT",
                c.pal.rule,
                5.4 * k,
            );
            let _ = t;
        }
    });
    let s = &d.story;
    let mut cells: Vec<(&str, &str, String)> = vec![
        ("GENDER", "story.gender", s.gender.clone()),
        ("AGE", "story.age", s.age.clone()),
        ("HEIGHT", "story.height", s.height.clone()),
        ("WEIGHT", "story.weight", s.weight.clone()),
        ("EYES", "story.eyes", s.eyes.clone()),
        ("SKIN", "story.skin", s.skin.clone()),
        ("HAIR", "story.hair", s.hair.clone()),
    ];
    match kind {
        AppKind::Details => cells.push(("DEITY", id::CHAR_DEITY, d.identity.deity.clone())),
        AppKind::Story => cells.push(("ALIGNMENT", "", d.identity.alignment.clone())),
    }
    let gx = x + pw + 8.0;
    let cw = (w - pw - 8.0 - 6.0) / 2.0;
    let t = Txt::new(8.0 * k, Style::Sans, cx.c.pal.ink);
    for (i, (label, fid, value)) in cells.iter().enumerate() {
        let (col, row) = (i % 2, i / 2);
        let cxp = gx + col as f32 * (cw + 6.0);
        let cyp = top + row as f32 * (ph / 4.0);
        cx.c.label(cxp + 1.0, cyp + 5.0, Align::Start, label);
        let r = (cxp, cyp + 7.0, cw, 15.0);
        if fid.is_empty() {
            cx.c.card(r, 2.5, Some(cx.c.pal.tint));
            cx.c.put(cxp + 3.0, cyp + 17.6, &t.within(cw - 6.0), value);
        } else {
            let fid = cx.ids.make(fid, cx.ordinal);
            cx.c.tinted_field(&fid, label, r, cyp + 17.6, value, &t);
        }
    }
}

/// Draw `p` (height `h`) at `(x, y)`.
fn draw(cx: &mut Cx, p: &Piece, (x, y, w): (f32, f32, f32), h: f32) {
    if !matches!(p, Piece::EquipRow(_)) {
        cx.c.close_table();
    }
    match p {
        Piece::Heading { title, hint } => {
            cx.c.heading(x, y, w, title, hint.as_deref());
        }
        Piece::Feature(f) => draw_feature(cx, f, (x, y, w)),
        Piece::Region(r) => draw_region(cx, r, (x, y, w), h),
        Piece::EquipHead { title, hint } => draw_equip_head(cx, title, hint.as_deref(), (x, y, w)),
        Piece::EquipRow(i) => draw_equip_row(cx, *i, (x, y, w)),
        Piece::Appearance(kind) => draw_appearance(cx, *kind, (x, y, w)),
    }
}

/// Height of the whole queue in one column of width `w` (gaps after table rows included).
fn queue_h(q: &VecDeque<Piece>, k: f32, w: f32) -> f32 {
    let mut h = 0.0;
    let mut after_row = false;
    for p in q {
        let is_row = matches!(p, Piece::EquipRow(_));
        if after_row && !is_row {
            h += AFTER_TABLE * k;
        }
        after_row = is_row;
        h += height(p, k, w);
    }
    h
}

/// [`fill`] for the last column a queue gets on its page: when the queue will not fit, room is
/// kept at the bottom for a "continued on page N" note, recorded in `marks`.
pub(crate) fn fill_last(
    cx: &mut Cx,
    q: &mut VecDeque<Piece>,
    (x, y0, w): (f32, f32, f32),
    bottom: f32,
    marks: &mut Vec<ContMark>,
) -> f32 {
    let k = cx.c.k;
    if queue_h(q, k, w) <= bottom - y0 + EPS {
        return fill(cx, q, (x, y0, w), bottom);
    }
    let y = fill(cx, q, (x, y0, w), bottom - MARK_H * k);
    if !q.is_empty() {
        marks.push(ContMark {
            page: cx.page_idx,
            x_right: x + w,
            base: bottom - 1.5 * k,
        });
    }
    y
}

/// Pour pieces from the front of `q` into the column `(x, y0, w)` down to `bottom`. Returns the
/// y below the last piece. Stops at the first piece that does not fit (it stays in `q`).
pub(crate) fn fill(
    cx: &mut Cx,
    q: &mut VecDeque<Piece>,
    (x, y0, w): (f32, f32, f32),
    bottom: f32,
) -> f32 {
    let k = cx.c.k;
    let mut y = y0;
    let mut after_row = false;
    while let Some(p) = q.pop_front() {
        let is_row = matches!(p, Piece::EquipRow(_));
        if after_row && !is_row && y > y0 + EPS {
            y += AFTER_TABLE * k;
        }
        after_row = is_row;
        let h = height(&p, k, w);
        let room = bottom - y;
        let keeps_next = matches!(p, Piece::Heading { .. } | Piece::EquipHead { .. });
        let need = h + if keeps_next {
            q.front().map_or(0.0, |n| min_h(n, k, w))
        } else {
            0.0
        };
        let at_top = y <= y0 + EPS;
        if need <= room + EPS {
            let grow = matches!(&p, Piece::Region(r) if r.fill) && q.is_empty();
            let h = if grow { room.max(h) } else { h };
            draw(cx, &p, (x, y, w), h);
            y += h;
            continue;
        }
        if min_h(&p, k, w) <= room + EPS || at_top {
            if let Some((head, tail)) = split(&p, k, w, room) {
                let hh = height(&head, k, w);
                draw(cx, &head, (x, y, w), hh);
                y += hh;
                q.push_front(tail);
                break;
            }
        }
        if at_top {
            draw(cx, &p, (x, y, w), h);
            y += h;
            continue;
        }
        q.push_front(p);
        break;
    }
    cx.c.close_table();
    y
}
