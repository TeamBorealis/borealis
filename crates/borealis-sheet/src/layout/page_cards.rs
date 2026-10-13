//! Card pages: spell, attack, item and feature cards in a 3x3 grid per page.
//!
//! The cards come from [`crate::card_model::pages`] (nine to a page, group order and page breaks
//! already applied). Every card text is a form field (`card.<slot>.<part>`, suffix `@<page>` from
//! the second card page on) so a fillable sheet can be edited; a flattened sheet draws the same
//! text. Titles shrink from 10 pt, descriptions from 6 pt down to a floor; a description that
//! still does not fit goes on in a "<title> (cont.)" card right after it.

use super::chrome;
use super::grid::{fit_size, wrap_lines, Align, Canvas, Grid, Palette, Rect, Txt, MARGIN_X};
use super::LayoutOptions;
use crate::card_model::Card;
use crate::fonts::{text_width, Style};
use crate::model_types::SheetData;
use crate::pdf::{PageOp, PdfPage, BASELINE};
use crate::pdf_draw::typographic_minus;
use crate::pdf_form::{FieldKind, FormField, TextAlign, TextField};

/// Gap between cards.
const GAP: f32 = 6.0;
/// Inner padding of a card.
const PAD: f32 = 5.0;
const TITLE_SIZE: f32 = 10.0;
const TITLE_MIN: f32 = 6.0;
const SUB_SIZE: f32 = 6.2;
const FACT_SIZE: f32 = 6.5;
const BODY_SIZE: f32 = 6.0;
const BODY_MIN: f32 = 5.4;
const FOOT_SIZE: f32 = 5.8;
const BODY_LEADING: f32 = 1.25;
const CORNER: f32 = 4.0;
/// Height reserved for the footer line.
const FOOT_H: f32 = 10.0;
/// Height of the "Cards" page heading (see `Canvas::heading`) and the gap below it.
const HEADING_H: f32 = 17.5;
const HEADING_GAP: f32 = 2.0;
/// Cards per page.
const PER_PAGE: usize = 9;
/// Continuation cards after one card, at most (a runaway text still ends in an ellipsis).
const MAX_CONT: usize = 8;

/// Field id of one card text: `card.3.title`, `card.3.title@2`.
fn fid(slot: usize, part: &str, page: usize) -> String {
    if page == 0 {
        format!("card.{}.{part}", slot + 1)
    } else {
        format!("card.{}.{part}@{}", slot + 1, page + 1)
    }
}

/// `text` cut with an ellipsis so it fits `max` at `size`.
fn clip(style: Style, size: f32, max: f32, text: &str) -> String {
    if text_width(style, text, size) <= max {
        return text.to_string();
    }
    let mut out: Vec<char> = text.chars().collect();
    while !out.is_empty() {
        let s = out.iter().collect::<String>().trim_end().to_string() + "\u{2026}";
        if text_width(style, &s, size) <= max {
            return s;
        }
        out.pop();
    }
    String::new()
}

/// Paragraph-aware wrap: one entry per printed line (blank for an empty paragraph).
fn wrap_body(size: f32, max: f32, text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let w = wrap_lines(Style::Sans, size, max, para);
        if w.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(w);
        }
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// Top of the card grid and the size of one card.
fn grid_geometry(g: &Grid, k: f32) -> (f32, f32, f32) {
    let top = g.body_top + HEADING_H * k + HEADING_GAP;
    let cw = (g.content_w() - 2.0 * GAP) / 3.0;
    let ch = (g.body_bottom - top - 2.0 * GAP) / 3.0;
    (top, cw, ch)
}

/// Offset of the description from the card top, as [`card`] lays it out.
fn body_offset(card: &Card, inner: f32) -> f32 {
    let tsize = fit_size(Style::SerifBold, &card.title, TITLE_SIZE, TITLE_MIN, inner);
    let mut y = PAD + tsize * 1.25 + 1.0 + SUB_SIZE * 1.4 + 2.0 + 3.0;
    y += card.facts.len() as f32 * FACT_SIZE * 1.35;
    if !card.facts.is_empty() {
        y += 2.0;
    }
    y
}

/// Width and height of the description box of `card` in a `w` x `h` card.
fn body_box(card: &Card, (w, h): (f32, f32)) -> (f32, f32) {
    let inner = w - 2.0 * PAD;
    let foot_y = h - PAD - FOOT_H;
    let body_h = (foot_y - 2.0 - body_offset(card, inner)).max(FOOT_SIZE);
    (inner - 2.0, body_h)
}

/// Whole description lines a box of height `h` shows at `size`.
fn lines_fitting(size: f32, h: f32) -> usize {
    let by_leading = (h / (size * BODY_LEADING)).floor().max(0.0) as usize;
    let by_box = crate::pdf_form::fitting_lines(
        BASELINE * size + 0.4,
        size * BODY_LEADING,
        size,
        h.max(size * 1.4),
    );
    by_leading.min(by_box)
}

/// `card` plus the "(cont.)" cards its description needs when it does not fit a `size` card
/// even at the smallest type.
fn with_continuations(card: &Card, size: (f32, f32)) -> Vec<Card> {
    let mut out = Vec::new();
    let mut cur = card.clone();
    for _ in 0..MAX_CONT {
        let (bw, bh) = body_box(&cur, size);
        if !fit_body(&cur.body, bw, bh).2 {
            break;
        }
        let lines = wrap_body(BODY_SIZE, bw, &cur.body);
        let n = lines_fitting(BODY_SIZE, bh).max(1);
        let rest = lines[n..]
            .iter()
            .skip_while(|l| l.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if rest.is_empty() {
            break;
        }
        let next = Card {
            title: format!("{} (cont.)", card.title),
            subtitle: card.subtitle.clone(),
            facts: Vec::new(),
            body: rest,
            left_footer: card.left_footer.clone(),
            right_footer: card.right_footer.clone(),
        };
        out.push(Card {
            body: lines[..n].join("\n"),
            ..cur
        });
        cur = next;
    }
    out.push(cur);
    out
}

/// The card pages with continuation cards added, nine to a page (a group page break of the
/// card model stays a page break).
fn paginate(pages: &[Vec<Card>], size: (f32, f32)) -> Vec<Vec<Card>> {
    pages
        .iter()
        .flat_map(|page| {
            let cards: Vec<Card> = page
                .iter()
                .flat_map(|c| with_continuations(c, size))
                .collect();
            cards
                .chunks(PER_PAGE)
                .map(<[Card]>::to_vec)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The size, wrapped lines and whether they were cut, for a body that must fit `lines_fit`.
fn fit_body(text: &str, max: f32, avail_h: f32) -> (f32, Vec<String>, bool) {
    let mut size = BODY_SIZE;
    loop {
        let lines = wrap_body(size, max, text);
        let fit = (avail_h / (size * BODY_LEADING)).floor().max(0.0) as usize;
        if lines.len() <= fit || size <= BODY_MIN {
            let cut = lines.len() > fit;
            return (size, lines, cut);
        }
        size = (size - 0.2).max(BODY_MIN);
    }
}

struct Ctx<'a> {
    fillable: bool,
    page: usize,
    slot: usize,
    pal: &'a Palette,
}

/// One single-line text, as a field.
fn line(c: &mut Canvas, cx: &Ctx<'_>, part: &str, tip: &str, at: Rect, t: &Txt, text: &str) {
    if text.is_empty() && !cx.fillable {
        return;
    }
    let (x, y, w, h) = at;
    let base = y + h - 2.0;
    c.text_field(
        &fid(cx.slot, part, cx.page),
        tip,
        (x, y, w, h),
        base,
        text,
        t,
    );
}

fn body_field(c: &mut Canvas, cx: &Ctx<'_>, (x, y, w, h): Rect, card: &Card) {
    let inner = w - 2.0;
    let (size, lines, cut) = fit_body(&card.body, inner, h);
    // Printed in both modes (a prefilled value is never a widget), so cut to the box.
    let value = {
        let mut shown: Vec<String> = lines
            .iter()
            .take(crate::pdf_form::fitting_lines(
                BASELINE * size + 0.4,
                size * BODY_LEADING,
                size,
                h.max(size * 1.4),
            ))
            .cloned()
            .collect();
        if cut {
            if let Some(last) = shown.last_mut() {
                *last = clip(Style::Sans, size, inner, &format!("{last} \u{2026}"));
            }
        }
        typographic_minus(&shown.join("\n")).into_owned()
    };
    if value.is_empty() && !cx.fillable {
        return;
    }
    let mut tf = TextField::new(&value, size);
    tf.style = Style::Sans;
    tf.color = cx.pal.ink;
    tf.align = TextAlign::Start;
    tf.multiline = true;
    tf.leading = Some(size * BODY_LEADING);
    tf.pad = 1.0;
    tf.baseline = Some(BASELINE * size + 0.4);
    let f = FormField::new(
        &fid(cx.slot, "text", cx.page),
        x,
        y,
        w,
        h.max(size * 1.4),
        FieldKind::Text(tf),
    )
    .with_tooltip("Card description");
    c.push(PageOp::Field(f));
}

/// Draw one card in `r`.
fn card(c: &mut Canvas, cx: &Ctx<'_>, r: Rect, card: &Card) {
    let (x0, y0, w, h) = r;
    c.card(r, CORNER, None);
    let x = x0 + PAD;
    let inner = w - 2.0 * PAD;
    let ink = c.pal.ink;
    let ink2 = c.pal.ink2;
    let mut y = y0 + PAD;
    // Title (serif, shrinks) and subtitle.
    let tsize = fit_size(Style::SerifBold, &card.title, TITLE_SIZE, TITLE_MIN, inner);
    let th = tsize * 1.25;
    line(
        c,
        cx,
        "title",
        "Card title",
        (x, y, inner, th),
        &Txt::new(tsize, Style::SerifBold, ink),
        &card.title,
    );
    y += th + 1.0;
    let sub = clip(Style::SansItalic, SUB_SIZE, inner, &card.subtitle);
    line(
        c,
        cx,
        "subtitle",
        "Card subtitle",
        (x, y, inner, SUB_SIZE * 1.4),
        &Txt::new(SUB_SIZE, Style::SansItalic, ink2),
        &sub,
    );
    y += SUB_SIZE * 1.4 + 2.0;
    c.hline(x, x + inner, y, c.pal.rule, 0.6);
    y += 3.0;
    for (i, (k, v)) in card.facts.iter().enumerate() {
        let text = clip(Style::Sans, FACT_SIZE, inner, &format!("{k}: {v}"));
        line(
            c,
            cx,
            &format!("fact{}", i + 1),
            "Card fact",
            (x, y, inner, FACT_SIZE * 1.35),
            &Txt::new(FACT_SIZE, Style::Sans, ink),
            &text,
        );
        y += FACT_SIZE * 1.35;
    }
    if !card.facts.is_empty() {
        y += 2.0;
    }
    // Footer rule, then the footers; the body takes the space between.
    let foot_y = y0 + h - PAD - FOOT_H;
    let body_h = (foot_y - 2.0 - y).max(FOOT_SIZE);
    body_field(c, cx, (x, y, inner, body_h), card);
    c.hline(x, x + inner, foot_y - 1.0, c.pal.rule, 0.6);
    let half = inner / 2.0 - 2.0;
    let left = clip(Style::SansItalic, FOOT_SIZE, half, &card.left_footer);
    let right = clip(Style::SansItalic, FOOT_SIZE, half, &card.right_footer);
    line(
        c,
        cx,
        "left",
        "Card footer, left",
        (x, foot_y, half, FOOT_H),
        &Txt::new(FOOT_SIZE, Style::SansItalic, ink2),
        &left,
    );
    line(
        c,
        cx,
        "right",
        "Card footer, right",
        (x + inner - half, foot_y, half, FOOT_H),
        &Txt::new(FOOT_SIZE, Style::SansItalic, ink2).align(Align::End),
        &right,
    );
}

/// Build the card pages: `pages` holds at most nine cards each.
pub(crate) fn build(
    pages: &[Vec<Card>],
    data: &SheetData,
    opts: &LayoutOptions,
    g: &Grid,
) -> Vec<PdfPage> {
    let k = if opts.compact {
        super::grid::COMPACT_K
    } else {
        1.0
    };
    let (_, cw, ch) = grid_geometry(g, k);
    paginate(pages, (cw, ch))
        .iter()
        .enumerate()
        .map(|(page, cards)| build_page(cards, data, opts, g, page))
        .collect()
}

fn build_page(
    cards: &[Card],
    data: &SheetData,
    opts: &LayoutOptions,
    g: &Grid,
    page: usize,
) -> PdfPage {
    let mut c = Canvas::new(Palette::new(opts.ink), opts.compact, g.w - MARGIN_X);
    chrome::furniture(&mut c, g, opts);
    chrome::header(&mut c, g, data);
    c.heading(MARGIN_X, g.body_top, g.content_w(), "Cards", None);
    let (top, cw, ch) = grid_geometry(g, c.k);
    let pal = c.pal;
    for (slot, k) in cards.iter().enumerate() {
        let col = (slot % 3) as f32;
        let row = (slot / 3) as f32;
        let r = (MARGIN_X + col * (cw + GAP), top + row * (ch + GAP), cw, ch);
        let cx = Ctx {
            fillable: opts.fillable,
            page,
            slot,
            pal: &pal,
        };
        card(&mut c, &cx, r, k);
    }
    PdfPage {
        ops: c.ops,
        struct_parents: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(body: &str, facts: usize) -> Card {
        Card {
            title: "Spellcasting".into(),
            subtitle: "Wizard 1".into(),
            facts: (0..facts).map(|i| (format!("F{i}"), "x".into())).collect(),
            body: body.into(),
            left_footer: "Wizard".into(),
            right_footer: "PHB".into(),
        }
    }

    const SIZE: (f32, f32) = (170.0, 200.0);

    #[test]
    fn a_short_description_needs_no_continuation() {
        assert_eq!(with_continuations(&card("Short.", 2), SIZE).len(), 1);
    }

    #[test]
    fn a_long_description_goes_on_in_cont_cards_and_loses_no_word() {
        let words: Vec<String> = (0..600).map(|i| format!("w{i}")).collect();
        let c = card(&words.join(" "), 2);
        let cards = with_continuations(&c, SIZE);
        assert!(cards.len() > 1);
        assert!(cards[1..].iter().all(|k| k.title == "Spellcasting (cont.)"));
        assert!(cards[1..].iter().all(|k| k.facts.is_empty()));
        for k in &cards {
            let (bw, bh) = body_box(k, SIZE);
            assert!(
                !fit_body(&k.body, bw, bh).2,
                "every card shows its whole part"
            );
        }
        let joined: Vec<String> = cards
            .iter()
            .flat_map(|k| {
                k.body
                    .split_whitespace()
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(joined, words);
    }

    #[test]
    fn continuations_follow_their_card_and_pages_hold_nine() {
        let long = "word ".repeat(900);
        let page: Vec<Card> = (0..9)
            .map(|i| card(if i == 4 { &long } else { "x" }, 0))
            .collect();
        let pages = paginate(&[page], SIZE);
        assert_eq!(pages[0].len(), 9);
        assert!(pages[0][5].title.ends_with("(cont.)"));
        let total: usize = pages.iter().map(Vec::len).sum();
        assert!(total > 9 && pages.len() == 2);
    }
}
