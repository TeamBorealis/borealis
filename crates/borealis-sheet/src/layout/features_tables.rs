//! Tables inside feature descriptions (B-21 M6), drawn as ruled tables with a header row below
//! the feature's text: column widths from the cell text, cells wrapped, rows split between pages
//! with the header row repeated. A table that does not fit the column (a word wider than its
//! column) stays as row sentences in the text field.
//!
//! The feature's text field holds the prose before the first table; prose that follows a table
//! in the description is printed below it, in a text field of its own (`feat.N.text.2`, the
//! way a split feature's continuation is). Table cells are printed text, tagged
//! `Table > TR > TH/TD`.

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
/// Line pitch of a header label wrapped over several lines.
const HEAD_LEAD: f32 = HEAD_SIZE * 1.25;
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

/// Widest word of a caps header label.
fn longest_head_word(text: &str, size: f32) -> f32 {
    head_label(text)
        .split_whitespace()
        .map(|w| tracked_width(Style::SansBold, w, size))
        .fold(0.0, f32::max)
}

/// Column widths for a table in width `w`, `None` when it has no room (a word wider than its
/// column). Header labels stay on one line when every one fits; otherwise they wrap.
pub(crate) fn widths(header: &[String], rows: &[Vec<String>], k: f32, w: f32) -> Option<Vec<f32>> {
    let n = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if n == 0 {
        return None;
    }
    let (hs, cs) = (HEAD_SIZE * k, CELL_SIZE * k);
    let col = |i: usize| rows.iter().map(move |r| cell(r, i));
    let words: Vec<f32> = (0..n)
        .map(|i| {
            col(i)
                .map(|t| longest_word(Style::Sans, cs, t))
                .fold(0.0, f32::max)
        })
        .collect();
    let with_head = |head: &dyn Fn(&str) -> f32| -> Vec<f32> {
        (0..n)
            .map(|i| head(cell(header, i)).max(words[i]) + 2.0 * PAD_X)
            .collect()
    };
    let one_line = with_head(&|h| tracked_width(Style::SansBold, &head_label(h), hs));
    let min = if one_line.iter().sum::<f32>() <= w {
        one_line
    } else {
        with_head(&|h| longest_head_word(h, hs))
    };
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

/// The caps header label of column `i` in lines that fit the column (one line when it fits).
fn head_lines(header: &[String], i: usize, k: f32, col_w: f32) -> Vec<String> {
    let (size, room) = (HEAD_SIZE * k, col_w - 2.0 * PAD_X + 0.01);
    let mut out: Vec<String> = Vec::new();
    for word in head_label(cell(header, i)).split_whitespace() {
        match out.last_mut() {
            Some(line)
                if tracked_width(Style::SansBold, &format!("{line} {word}"), size) <= room =>
            {
                line.push(' ');
                line.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out
}

/// Height of the header row: one label line, plus a line pitch per extra wrapped line.
fn head_h(t: &TablePiece, ws: &[f32], k: f32) -> f32 {
    let n = (0..ws.len())
        .map(|i| head_lines(&t.header, i, k, ws[i]).len())
        .max()
        .unwrap_or(1)
        .max(1);
    (HEAD_ROW_H + (n - 1) as f32 * HEAD_LEAD) * k
}

fn top_h(t: &TablePiece, ws: &[f32], k: f32) -> f32 {
    (if t.cont { CONT_H } else { 0.0 }) * k + head_h(t, ws, k)
}

/// Height of the header and `rows` (the whole table when it has no room: nothing is drawn).
fn body_h(t: &TablePiece, rows: &[Vec<String>], k: f32, w: f32) -> f32 {
    let Some(ws) = widths(&t.header, &t.rows, k, w) else {
        return (if t.cont { CONT_H } else { 0.0 } + HEAD_ROW_H) * k;
    };
    top_h(t, &ws, k) + rows.iter().map(|r| row_h(r, &ws, k)).sum::<f32>()
}

/// Height of the table piece in a column of width `w`.
pub(crate) fn height(t: &TablePiece, k: f32, w: f32) -> f32 {
    body_h(t, &t.rows, k, w) + AFTER * k
}

/// Smallest height of a head part (the header and the first rows).
pub(crate) fn min_height(t: &TablePiece, k: f32, w: f32) -> f32 {
    if t.rows.len() <= 2 * MIN_ROWS {
        return height(t, k, w);
    }
    body_h(t, &t.rows[..MIN_ROWS], k, w) + AFTER * k
}

/// Split between rows so the head fits `room`; both parts keep at least [`MIN_ROWS`] rows.
pub(crate) fn split(t: &TablePiece, k: f32, w: f32, room: f32) -> Option<(TablePiece, TablePiece)> {
    let ws = widths(&t.header, &t.rows, k, w)?;
    let mut used = top_h(t, &ws, k) + AFTER * k;
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
            let (base, label) = (top + 6.5 * k, head_label(cell(&t.header, i)));
            let lines = head_lines(&t.header, i, k, ws[i]);
            if lines.len() > 1 {
                c.label_lines(cx + PAD_X, base, HEAD_LEAD * k, &lines);
            } else {
                c.label(cx + PAD_X, base, Align::Start, &label);
            }
        }
    });
    top += head_h(t, &ws, k);
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

/// The prose of `text` around `tables` (their row sentences left out): the paragraphs before
/// the first table, then the paragraphs after each table up to the next one (blank paragraphs
/// at either end of a later segment dropped). One more segment than tables.
pub(crate) fn prose_segments(text: &str, tables: &[&TextTable]) -> Vec<String> {
    let mut starts: Vec<&TextTable> = tables.to_vec();
    starts.sort_by_key(|t| t.lines.start);
    let mut segments: Vec<Vec<&str>> = vec![Vec::new(); starts.len() + 1];
    for (i, line) in text.split('\n').enumerate() {
        if starts.iter().any(|t| t.lines.contains(&i)) {
            continue;
        }
        let seg = starts.iter().filter(|t| t.lines.start <= i).count();
        segments[seg].push(line);
    }
    segments
        .into_iter()
        .enumerate()
        .map(|(n, lines)| {
            let joined = lines.join("\n");
            if n == 0 {
                joined
            } else {
                joined.trim_matches('\n').to_string()
            }
        })
        .collect()
}

/// The text of feature `f` before its first table drawn as a table in width `w`, and those
/// tables in order, each with the prose that follows it (printed below the table). Tables
/// without room keep their rows in the text.
pub(crate) fn split_off_tables(
    f: &FeatureData,
    k: f32,
    w: f32,
) -> (String, Vec<(TablePiece, String)>) {
    let mut drawn: Vec<&TextTable> = f
        .tables
        .iter()
        .filter(|t| widths(&t.header, &t.rows, k, w).is_some())
        .collect();
    if drawn.is_empty() {
        return (f.text.clone(), Vec::new());
    }
    drawn.sort_by_key(|t| t.lines.start);
    let mut segments = prose_segments(&f.text, &drawn).into_iter();
    let text = segments.next().unwrap_or_default();
    let pieces = drawn
        .into_iter()
        .zip(segments)
        .map(|(t, after)| {
            let piece = TablePiece {
                name: f.name.clone(),
                header: t.header.clone(),
                rows: t.rows.clone(),
                cont: false,
                group: String::new(),
            };
            (piece, after)
        })
        .collect();
    (text, pieces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_wrap_only_when_one_line_does_not_fit() {
        let head: Vec<String> = (0..3)
            .map(|c| format!("Extraordinarily long heading label {c}"))
            .collect();
        let rows = vec![vec!["a".to_string(), "b".into(), "c".into()]];
        let one_line: f32 = head
            .iter()
            .map(|h| tracked_width(Style::SansBold, &head_label(h), HEAD_SIZE) + 2.0 * PAD_X)
            .sum();
        let w = 240.0;
        assert!(
            one_line > w,
            "the labels do not fit on one line ({one_line})"
        );
        let ws = widths(&head, &rows, 1.0, w).expect("drawn with wrapped headers");
        assert!(head_lines(&head, 0, 1.0, ws[0]).len() > 1);
        let short: Vec<String> = ["A", "B", "C"].map(String::from).to_vec();
        let ws = widths(&short, &rows, 1.0, w).unwrap();
        assert_eq!(head_lines(&short, 0, 1.0, ws[0]), ["A"]);
    }

    #[test]
    fn prose_segments_follow_their_tables() {
        let t = |lines: std::ops::Range<usize>| TextTable {
            header: Vec::new(),
            rows: Vec::new(),
            lines,
        };
        let (a, b) = (t(1..3), t(4..5));
        let text = "before\nrow\nrow\nbetween\nrow\n\nafter";
        assert_eq!(
            prose_segments(text, &[&b, &a]),
            ["before", "between", "after"]
        );
    }
}
