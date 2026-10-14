//! Reading order of a page: blocks stacked over the columns, then columns found recursively
//! (two levels: up to four columns), tables across a gutter read row by row, and two-column
//! tables inside a column of prose read row by row.

use super::super::pdf_table::{paired_rows, spanning_tables, tables};
use super::{Fragment, SAME_LINE};

/// Column splits followed (two levels: up to four columns).
const MAX_COLUMN_SPLITS: usize = 2;
/// Share of row labels (numbers, dice) among the left lines of a column that make them a
/// table's first column rather than a column of their own.
const NUMBERED_SHARE: f32 = 0.8;
/// Where a column gutter is looked for (shares of the page width).
const GUTTER_RANGE: (f32, f32) = (0.3, 0.7);
/// A full-width line reaches this share of the page width into both halves.
const FULL_REACH: f32 = 0.15;
/// Fragments on each side of the middle before a page counts as two columns.
const MIN_COLUMN_LINES: usize = 3;
/// A line this much larger than a column's text, at its left margin, is a heading of the
/// column, not a table cell.
const MARGIN_HEADING_RATIO: f32 = 1.12;
/// Lines of a wrapped cell looked past for the row above.
const WRAPPED_LINES: usize = 3;
/// Font sizes from a column's left margin that a heading still starts at it.
const MARGIN_SLACK: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Full,
}

/// Reading order: one column top to bottom, or, on a page of columns, each band between
/// full-width lines read column by column (columns found recursively, so three columns or
/// unequal columns read in order too).
pub(super) fn reading_order(frags: Vec<Fragment>, width: f32) -> Vec<Fragment> {
    order_columns(frags, 0.0, width, MAX_COLUMN_SPLITS)
}

fn order_columns(frags: Vec<Fragment>, x0: f32, x1: f32, depth: usize) -> Vec<Fragment> {
    // Blocks stacked over all the columns are read one after the other.
    let frags = if depth == 0 {
        frags
    } else {
        match block_cut(frags, x0, x1) {
            Ok((above, below)) => {
                let mut out = order_columns(above, x0, x1, depth);
                out.extend(order_columns(below, x0, x1, depth));
                return out;
            }
            Err(frags) => frags,
        }
    };
    let width = (x1 - x0).max(1.0);
    let mid = gutter(&frags, x0, width);
    let slack = 2.0;
    // Full width: well into both halves. A cell line slightly over the gutter keeps the side
    // most of it is on.
    let placed = |f: &Fragment| {
        if f.end <= mid + slack {
            Side::Left
        } else if f.x >= mid - slack {
            Side::Right
        } else if f.x < mid - FULL_REACH * width && f.end > mid + FULL_REACH * width {
            Side::Full
        } else if (f.x + f.end) / 2.0 < mid {
            Side::Left
        } else {
            Side::Right
        }
    };
    // Inside a column with lines across it (a table set in a column of prose), a heading at
    // the column's left margin with nothing beside it ends the table: it is not a cell of the
    // table's first column.
    let in_column = depth < MAX_COLUMN_SPLITS && frags.iter().any(|f| placed(f) == Side::Full);
    let margin = usual_edge(&frags);
    let body = body_size(&frags);
    let alone = |f: &Fragment| {
        !frags
            .iter()
            .any(|g| g.x > f.end && (g.y - f.y).abs() <= f.size.max(1.0))
    };
    let breaks: Vec<(f32, f32)> = frags
        .iter()
        .filter(|f| {
            in_column
                && f.size >= MARGIN_HEADING_RATIO * body
                && (f.x - margin).abs() <= MARGIN_SLACK * f.size.max(1.0)
                && alone(f)
        })
        .map(|f| (f.x, f.y))
        .collect();
    let side = |f: &Fragment| {
        if breaks.contains(&(f.x, f.y)) {
            Side::Full
        } else {
            placed(f)
        }
    };
    let side_of = |f: &Fragment| match side(f) {
        Side::Left => Some(true),
        Side::Right => Some(false),
        Side::Full => None,
    };
    let spans = spanning_tables(&frags, &side_of);
    let in_span = |f: &Fragment| {
        let slack = SAME_LINE * f.size.max(1.0);
        spans
            .iter()
            .any(|(top, bottom)| f.y <= top + slack && f.y >= bottom - slack)
    };
    let count = |s: Side| frags.iter().filter(|f| !in_span(f) && side(f) == s).count();
    // Inside a column, a left side of numbers ("1", "01-50", "d20") is a table's first column.
    let lefts: Vec<&Fragment> = frags.iter().filter(|f| side(f) == Side::Left).collect();
    let numbered = depth < MAX_COLUMN_SPLITS
        && lefts.iter().filter(|f| row_label(&f.text)).count() as f32
            >= NUMBERED_SHARE * lefts.len() as f32;
    if depth == 0
        || numbered
        || count(Side::Left) < MIN_COLUMN_LINES
        || count(Side::Right) < MIN_COLUMN_LINES
    {
        return merge_rows(tables(frags));
    }
    let mut out = Vec::new();
    let (mut left, mut right, mut table) = (Vec::new(), Vec::new(), Vec::new());
    let flush = |out: &mut Vec<Fragment>, left: &mut Vec<Fragment>, right: &mut Vec<Fragment>| {
        // A two-column table in a column of prose: its sides pair up line by line.
        if in_column && paired_rows(left, right) {
            let mut band: Vec<Fragment> = std::mem::take(left);
            band.append(right);
            band.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
            out.extend(merge_rows(band));
            return;
        }
        out.extend(order_columns(std::mem::take(left), x0, mid, depth - 1));
        out.extend(order_columns(std::mem::take(right), mid, x1, depth - 1));
    };
    for f in frags {
        // A table across the gutter is read row by row, after the columns above it.
        if in_span(&f) {
            if table.is_empty() {
                flush(&mut out, &mut left, &mut right);
            }
            table.push(f);
            continue;
        }
        if !table.is_empty() {
            out.extend(merge_rows(tables(std::mem::take(&mut table))));
        }
        match side(&f) {
            Side::Left => left.push(f),
            Side::Right => right.push(f),
            Side::Full => {
                flush(&mut out, &mut left, &mut right);
                out.push(f);
            }
        }
    }
    out.extend(merge_rows(tables(table)));
    flush(&mut out, &mut left, &mut right);
    out
}

/// A table row label: a number, a number range or a die ("12", "01-50", "d20").
fn row_label(t: &str) -> bool {
    let t = t.trim();
    let t = t.strip_prefix(['d', 'D']).unwrap_or(t);
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '–' | '+'))
}

/// The x of the widest vertical white channel near the middle of the page (between two
/// columns, which need not be equal): the place fewest fragments cross, nearest the middle.
fn gutter(frags: &[Fragment], x0: f32, width: f32) -> f32 {
    let mid = x0 + width / 2.0;
    let lo = (x0 + width * GUTTER_RANGE.0) as i32;
    let hi = (x0 + width * GUTTER_RANGE.1) as i32;
    // Only places with columns on both sides count.
    let balanced = |x: f32| {
        let left = frags.iter().filter(|f| f.end <= x).count();
        let right = frags.iter().filter(|f| f.x >= x).count();
        left.min(right) >= MIN_COLUMN_LINES
    };
    let crossing = |x: f32| frags.iter().filter(|f| f.x < x && f.end > x).count();
    (lo..=hi)
        .map(|x| x as f32)
        .filter(|x| balanced(*x))
        .min_by(|a, b| {
            crossing(*a)
                .cmp(&crossing(*b))
                .then((a - mid).abs().total_cmp(&(b - mid).abs()))
        })
        .unwrap_or(mid)
}

/// Single-column page: fragments on one baseline (table cells) become one `a | b` line.
fn merge_rows(frags: Vec<Fragment>) -> Vec<Fragment> {
    let mut out: Vec<Fragment> = Vec::new();
    for f in frags {
        match out.last_mut() {
            Some(prev) if (prev.y - f.y).abs() <= SAME_LINE * prev.size.max(f.size).max(1.0) => {
                prev.text = format!("{} | {}", prev.text.trim_end(), f.text.trim_start());
                prev.end = f.end;
                prev.bold &= f.bold;
            }
            _ => out.push(f),
        }
    }
    split_label_cells(out)
}

/// In a table whose rows start with a row label alone ("01-50 | Iron | ..."), a row one cell
/// short whose first cell is a label and a word ("96-100 Adamantine | ...") had those two cells
/// set too close to part: they are split.
fn split_label_cells(rows: Vec<Fragment>) -> Vec<Fragment> {
    let first = |f: &Fragment| f.text.split(" | ").next().unwrap_or("").trim().to_string();
    let cells = |f: &Fragment| f.text.split(" | ").count();
    let split = |i: usize| -> Option<String> {
        let row = &rows[i];
        let head = first(row);
        let (label, rest) = head.split_once(' ')?;
        if !row.text.contains(" | ") || row_label(&head) || !row_label(label) {
            return None;
        }
        // The row above, past a wrapped cell's lines.
        let above = rows[..i]
            .iter()
            .rev()
            .take(WRAPPED_LINES + 1)
            .find(|r| r.text.contains(" | "));
        let fits = above.is_some_and(|r| row_label(&first(r)) && cells(r) == cells(row) + 1);
        fits.then(|| {
            let tail = row.text.trim_start()[head.len()..].to_string();
            format!("{label} | {}{tail}", rest.trim())
        })
    };
    let fixed: Vec<Option<String>> = (0..rows.len()).map(split).collect();
    rows.into_iter()
        .zip(fixed)
        .map(|(f, t)| match t {
            Some(text) => Fragment { text, ..f },
            None => f,
        })
        .collect()
}

// ---- stacked blocks -----------------------------------------------------------------------

/// A block heading is at least this many times the size most of the region's text has.
const BLOCK_RATIO: f32 = 1.8;
/// Fragments a part needs on each side of the region's middle to count as columns.
const MIN_SIDE_LINES: usize = 3;
/// Baselines this share of the heading size below it still sit on the heading's band.
const BAND_BELOW: f32 = 0.35;

/// The size most of the characters in `frags` have.
fn body_size(frags: &[Fragment]) -> f32 {
    let mut by_size: Vec<(i32, usize)> = Vec::new();
    for f in frags {
        let key = (f.size * 2.0).round() as i32;
        let n = f.text.chars().filter(|c| !c.is_whitespace()).count();
        match by_size.iter_mut().find(|(k, _)| *k == key) {
            Some(e) => e.1 += n,
            None => by_size.push((key, n)),
        }
    }
    by_size
        .iter()
        .max_by_key(|(_, n)| *n)
        .map_or(0.0, |(k, _)| *k as f32 / 2.0)
}

/// The left edge most lines start at (a column's margin), to the nearest point.
fn usual_edge(frags: &[Fragment]) -> f32 {
    let mut by_edge: Vec<(i32, usize)> = Vec::new();
    for f in frags {
        let key = f.x.round() as i32;
        match by_edge.iter_mut().find(|(k, _)| *k == key) {
            Some(e) => e.1 += 1,
            None => by_edge.push((key, 1)),
        }
    }
    by_edge
        .iter()
        .max_by_key(|(_, n)| *n)
        .map_or(0.0, |(k, _)| *k as f32)
}

/// Both sides of `mid` hold column lines.
fn two_sided(part: &[&Fragment], mid: f32) -> bool {
    let left = part.iter().filter(|f| f.end <= mid).count();
    let right = part.iter().filter(|f| f.x >= mid).count();
    left.min(right) >= MIN_SIDE_LINES
}

/// Splits `frags` (one region from `x0` to `x1`) above the first block heading: a fragment at
/// least [`BLOCK_RATIO`] times the body size with no other text on its band, under which the
/// text runs in columns on both sides of the middle, and over which it does too (or only lines
/// across the middle, a page title, stand). `None` when there is none, so
/// two-column prose with a heading in one column (the other column's lines pass beside it) and
/// a page whose second column ends above a heading stay one region.
fn block_cut(
    frags: Vec<Fragment>,
    x0: f32,
    x1: f32,
) -> Result<(Vec<Fragment>, Vec<Fragment>), Vec<Fragment>> {
    let body = body_size(&frags);
    let mid = (x0 + x1) / 2.0;
    let cut = frags
        .iter()
        .filter(|h| body > 0.0 && h.size >= BLOCK_RATIO * body)
        .map(|h| (h.y, h.size))
        .find(|&(y, size)| {
            let on_band = |f: &&Fragment| f.y < y + size && f.y > y - BAND_BELOW * size;
            if frags.iter().filter(on_band).count() > 1 {
                return false;
            }
            let above: Vec<&Fragment> = frags.iter().filter(|f| f.y >= y + size).collect();
            let below: Vec<&Fragment> = frags.iter().filter(|f| f.y < y).collect();
            let spans = |f: &&Fragment| f.x < mid && f.end > mid;
            let titled = !above.is_empty() && above.iter().all(spans);
            (two_sided(&above, mid) || titled) && two_sided(&below, mid)
        });
    let Some((y, _)) = cut else {
        return Err(frags);
    };
    Ok(frags.into_iter().partition(|f| f.y > y))
}
