//! Tables inside feature descriptions (B-21 M6), drawn as ruled tables with a header row below
//! the feature's text: column widths from the cell text, cells wrapped, rows split between pages
//! with the header row repeated. A table that does not fit the column (a word wider than its
//! column) stays as row sentences in the text field.
//!
//! The feature's text field holds the prose; the table follows it (prose that came after the
//! table in the description is printed above it, in the same field). Table cells are printed
//! text, tagged `Table > TR > TH/TD`.

use super::features_text::wrap_field;
use crate::fonts::{text_width, Style};
use crate::layout::grid::{tracked_width, Align, Canvas, Txt, LABEL_SIZE};
use crate::model_types::{FeatureData, TextTable};

/// A header cell as a caps label.
fn head_label(text: &str) -> String {
    text.to_uppercase()
}

/// Size of the header labels (the sheet's caps label size).
const HEAD_SIZE: f32 = LABEL_SIZE;
/// Size of the body cells.
const CELL_SIZE: f32 = 7.0;
/// Line pitch of wrapped cell text, as a multiple of the size.
const CELL_LEAD: f32 = 1.25;
/// Height of the header row.
const HEAD_ROW_H: f32 = 10.0;
/// Padding inside a cell, left and right.
const PAD_X: f32 = 2.0;
/// Space above the first and below the last line of a body row.
const PAD_Y: f32 = 1.5;
/// Space after the table.
const AFTER: f32 = 4.0;
/// Height of the "(continued)" line of a table that goes on on a new page.
const CONT_H: f32 = 9.0;
/// Fewest body rows of a table part (head or tail of a split).
const MIN_ROWS: usize = 2;

/// One table (or a part of one) in the feature flow.
#[derive(Debug, Clone)]
pub(crate) struct TablePiece {
    /// Feature name (for the "(continued)" line).
    pub name: String,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// A later part of a split table.
    pub cont: bool,
    /// Group heading of the feature (as `Feat::group`).
    pub group: String,
}

/// Cell `i` of `row` (empty when the row is short).
fn cell(row: &[String], i: usize) -> &str {
    row.get(i).map_or("", String::as_str)
}

fn longest_word(style: Style, size: f32, text: &str) -> f32 {
    text.split_whitespace()
        .map(|w| text_width(style, w, size))
        .fold(0.0, f32::max)
}

/// Column widths for a table in width `w`, `None` when it has no room (a word or a header wider
/// than its column).
pub(crate) fn widths(header: &[String], rows: &[Vec<String>], k: f32, w: f32) -> Option<Vec<f32>> {
    let n = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if n == 0 {
        return None;
    }
    let (hs, cs) = (HEAD_SIZE * k, CELL_SIZE * k);
    let col = |i: usize| rows.iter().map(move |r| cell(r, i));
    let min: Vec<f32> = (0..n)
        .map(|i| {
            let head = tracked_width(Style::SansBold, &head_label(cell(header, i)), hs);
            let words = col(i)
                .map(|t| longest_word(Style::Sans, cs, t))
                .fold(0.0, f32::max);
            head.max(words) + 2.0 * PAD_X
        })
        .collect();
    let natural: Vec<f32> = (0..n)
        .map(|i| {
            col(i)
                .map(|t| text_width(Style::Sans, t, cs))
                .fold(0.0, f32::max)
                + 2.0 * PAD_X
        })
        .zip(&min)
        .map(|(a, b)| a.max(*b))
        .collect();
    if min.iter().sum::<f32>() > w {
        return None;
    }
    Some(share(&min, &natural, w))
}

/// Column widths: columns whose text fits a fair share get their natural width (short label
/// columns do not wrap), the others share the rest in proportion to their text, never below
/// their minimum; any room left is spread evenly.
fn share(min: &[f32], natural: &[f32], w: f32) -> Vec<f32> {
    let n = min.len();
    let mut out: Vec<Option<f32>> = vec![None; n];
    loop {
        let open: Vec<usize> = (0..n).filter(|&i| out[i].is_none()).collect();
        if open.is_empty() {
            break;
        }
        let left = w - out.iter().flatten().sum::<f32>();
        let fair = left / open.len() as f32;
        let small: Vec<usize> = open
            .iter()
            .copied()
            .filter(|&i| natural[i] <= fair)
            .collect();
        if small.is_empty() {
            let spare = left - open.iter().map(|&i| min[i]).sum::<f32>();
            if spare < 0.0 {
                // The fitted columns left too little: share everything above the minimums.
                return proportional(min, natural, w);
            }
            let extra: f32 = open.iter().map(|&i| natural[i] - min[i]).sum();
            for &i in &open {
                let part = if extra > 0.0 {
                    (natural[i] - min[i]) / extra * spare
                } else {
                    spare / open.len() as f32
                };
                out[i] = Some(min[i] + part);
            }
            break;
        }
        for i in small {
            out[i] = Some(natural[i]);
        }
    }
    let widths: Vec<f32> = out.into_iter().map(Option::unwrap_or_default).collect();
    let spare = (w - widths.iter().sum::<f32>()).max(0.0) / n as f32;
    widths.into_iter().map(|c| c + spare).collect()
}

/// Minimum widths plus the room above them in proportion to the text each column would wrap.
fn proportional(min: &[f32], natural: &[f32], w: f32) -> Vec<f32> {
    let spare = w - min.iter().sum::<f32>();
    let extra: f32 = natural.iter().zip(min).map(|(a, b)| a - b).sum();
    min.iter()
        .zip(natural)
        .map(|(m, a)| {
            if extra > 0.0 {
                m + (a - m) / extra * spare
            } else {
                m + spare / min.len() as f32
            }
        })
        .collect()
}

fn lines(text: &str, k: f32, col_w: f32) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    wrap_field(Style::Sans, CELL_SIZE * k, col_w - 2.0 * PAD_X, text)
}

fn row_h(row: &[String], widths: &[f32], k: f32) -> f32 {
    let n = widths
        .iter()
        .enumerate()
        .map(|(i, cw)| lines(cell(row, i), k, *cw).len())
        .max()
        .unwrap_or(1)
        .max(1);
    n as f32 * CELL_SIZE * k * CELL_LEAD + 2.0 * PAD_Y
}

fn top_h(t: &TablePiece, k: f32) -> f32 {
    (if t.cont { CONT_H } else { 0.0 } + HEAD_ROW_H) * k
}

fn rows_h(t: &TablePiece, rows: &[Vec<String>], k: f32, w: f32) -> f32 {
    let Some(ws) = widths(&t.header, &t.rows, k, w) else {
        return 0.0;
    };
    rows.iter().map(|r| row_h(r, &ws, k)).sum()
}

/// Height of the table piece in a column of width `w`.
pub(crate) fn height(t: &TablePiece, k: f32, w: f32) -> f32 {
    top_h(t, k) + rows_h(t, &t.rows, k, w) + AFTER * k
}

/// Smallest height of a head part (the header and the first rows).
pub(crate) fn min_height(t: &TablePiece, k: f32, w: f32) -> f32 {
    if t.rows.len() <= 2 * MIN_ROWS {
        return height(t, k, w);
    }
    top_h(t, k) + rows_h(t, &t.rows[..MIN_ROWS], k, w) + AFTER * k
}

/// Split between rows so the head fits `room`; both parts keep at least [`MIN_ROWS`] rows.
pub(crate) fn split(t: &TablePiece, k: f32, w: f32, room: f32) -> Option<(TablePiece, TablePiece)> {
    let ws = widths(&t.header, &t.rows, k, w)?;
    let mut used = top_h(t, k) + AFTER * k;
    let mut fit = 0;
    for r in &t.rows {
        let h = row_h(r, &ws, k);
        if used + h > room + 0.01 {
            break;
        }
        used += h;
        fit += 1;
    }
    let fit = fit.min(t.rows.len().saturating_sub(MIN_ROWS));
    if fit < MIN_ROWS {
        return None;
    }
    let head = TablePiece {
        rows: t.rows[..fit].to_vec(),
        ..t.clone()
    };
    let tail = TablePiece {
        rows: t.rows[fit..].to_vec(),
        cont: true,
        ..t.clone()
    };
    Some((head, tail))
}

/// Draw the table at `(x, y)` in width `w`.
pub(crate) fn draw(c: &mut Canvas, t: &TablePiece, (x, y, w): (f32, f32, f32)) {
    let k = c.k;
    let Some(ws) = widths(&t.header, &t.rows, k, w) else {
        return;
    };
    let mut top = y;
    if t.cont {
        let note = Txt::new(7.0 * k, Style::SansItalic, c.pal.ink2).within(w);
        c.put(x, y + 7.0 * k, &note, &format!("{} (continued)", t.name));
        top += CONT_H * k;
    }
    let xs: Vec<f32> = ws
        .iter()
        .scan(x, |at, cw| {
            let left = *at;
            *at += cw;
            Some(left)
        })
        .collect();
    c.close_table();
    c.open_table();
    // Caps labels as the other sheet tables (`TH` cells in a header row).
    c.header_row(|c| {
        for (i, cx) in xs.iter().enumerate() {
            c.label(
                cx + PAD_X,
                top + 6.5 * k,
                Align::Start,
                &head_label(cell(&t.header, i)),
            );
        }
    });
    top += HEAD_ROW_H * k;
    c.hline(x, x + w, top, c.pal.ink, 0.9);
    let body = Txt::new(CELL_SIZE * k, Style::Sans, c.pal.ink);
    let lead = CELL_SIZE * k * CELL_LEAD;
    for row in &t.rows {
        let h = row_h(row, &ws, k);
        c.group("TR", |c| {
            for (i, cx) in xs.iter().enumerate() {
                let text = lines(cell(row, i), k, ws[i]);
                c.leaf("P", None, |c| {
                    for (j, line) in text.iter().enumerate() {
                        let base = top + PAD_Y + lead * j as f32 + CELL_SIZE * k;
                        c.put(cx + PAD_X, base, &body.align(Align::Start), line);
                    }
                });
            }
        });
        top += h;
        c.hline(x, x + w, top, c.pal.rule, 0.5);
    }
    c.close_table();
}

/// The text of feature `f` without the row sentences of the tables drawn as tables in width
/// `w`, and those tables in order. Tables without room keep their rows in the text.
pub(crate) fn split_off_tables(f: &FeatureData, k: f32, w: f32) -> (String, Vec<TablePiece>) {
    let drawn: Vec<&TextTable> = f
        .tables
        .iter()
        .filter(|t| widths(&t.header, &t.rows, k, w).is_some())
        .collect();
    if drawn.is_empty() {
        return (f.text.clone(), Vec::new());
    }
    let text = f
        .text
        .split('\n')
        .enumerate()
        .filter(|(i, _)| !drawn.iter().any(|t| t.lines.contains(i)))
        .map(|(_, l)| l)
        .collect::<Vec<_>>()
        .join("\n");
    let pieces = drawn
        .into_iter()
        .map(|t| TablePiece {
            name: f.name.clone(),
            header: t.header.clone(),
            rows: t.rows.clone(),
            cont: false,
            group: String::new(),
        })
        .collect();
    (text, pieces)
}
