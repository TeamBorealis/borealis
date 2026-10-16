//! Tables on a PDF page: cells are told apart by shared left edges (tab stops), stacked into
//! cells, and the cells of one row (vertically centred or not) are joined into one
//! `cell | cell | cell` line, so a row's item name stays whole and next to its text.

use super::pdf::Fragment;
use super::pdf_walk::Run;

// A child of this module so the import module list stays as it is.
#[path = "pdf_table_headed.rs"]
pub(crate) mod headed;

/// A run starting this many points from a stop starts at it.
const STOP_SLACK: f32 = 1.0;
/// Baselines on which a left edge must recur to be a tab stop.
const MIN_STOP_LINES: usize = 4;
/// A gap narrower than this many font sizes is a word space.
const PHRASE_GAP: f32 = 0.6;
/// Share of a stop's runs that follow one fixed word for the stop to be a repeated phrase.
const PHRASE_SHARE: f32 = 0.8;
/// The gap before a cell is at least this many font sizes (wider than a word space).
pub(crate) const CELL_GAP: f32 = 0.3;
/// Lines of one cell are at most this many font sizes apart.
const CELL_LEADING: f32 = 1.45;
/// A single fragment wider than this share of the table ends it (a prose line).
const WIDE_LINE: f32 = 0.6;
/// A line this much larger than a table's first line is a heading after it.
const HEADING_RATIO: f32 = 1.12;
/// Fewest rows a table has.
const MIN_ROWS: usize = 2;
/// Cells on a baseline that make it a full row of a dense table.
const DENSE_CELLS: usize = 3;
/// Share of a table's baselines that are full rows for it to be read one row per baseline.
const DENSE_SHARE: f32 = 0.6;

/// Where a run starts: x, baseline, and the gap (font sizes) after the previous run of its line
/// with that run's end.
type Start = (f32, f32, Option<(f32, f32)>);

/// Left edges where runs start on at least [`MIN_STOP_LINES`] baselines (runs sorted top down),
/// except where the run before always ends at the same place a word space away: the second
/// word of a repeated label ("Casting Time:") is not a column.
pub(crate) fn tab_stops(runs: &[Run]) -> Vec<f32> {
    let mut starts: Vec<Start> = Vec::with_capacity(runs.len());
    for (i, r) in runs.iter().enumerate() {
        let prev = i
            .checked_sub(1)
            .map(|k| &runs[k])
            .filter(|p| (p.y - r.y).abs() <= 0.35 * p.size.max(r.size).max(1.0) && p.x < r.x);
        starts.push((
            r.x,
            r.y,
            prev.map(|p| ((r.x - p.end) / r.size.max(1.0), p.end)),
        ));
    }
    starts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut stops = Vec::new();
    let mut i = 0;
    while i < starts.len() {
        let mut j = i;
        let mut ys: Vec<f32> = Vec::new();
        while j < starts.len() && starts[j].0 - starts[i].0 <= STOP_SLACK {
            // A run continuing a word or after a word space does not make a stop.
            let after_gap = starts[j].2.map_or(true, |(gap, _)| gap >= CELL_GAP);
            if after_gap && !ys.iter().any(|y| (y - starts[j].1).abs() < 1.0) {
                ys.push(starts[j].1);
            }
            j += 1;
        }
        if ys.len() >= MIN_STOP_LINES && !repeated_phrase(&starts[i..j]) {
            stops.push(starts[i].0);
        }
        i = j;
    }
    stops
}

/// Most of the runs at a stop follow a run ending at one place, a word space before them.
fn repeated_phrase(at: &[Start]) -> bool {
    let before: Vec<(f32, f32)> = at
        .iter()
        .filter_map(|s| s.2)
        .filter(|(gap, _)| *gap >= CELL_GAP)
        .collect();
    let same_end = |e: f32| {
        before
            .iter()
            .filter(|(gap, end)| (end - e).abs() <= STOP_SLACK && *gap < PHRASE_GAP)
            .count()
    };
    let most = before.iter().map(|(_, e)| same_end(*e)).max().unwrap_or(0);
    most >= 2 && most as f32 >= PHRASE_SHARE * before.len() as f32
}

pub(crate) fn at_stop(stops: &[f32], x: f32) -> bool {
    stops.iter().any(|s| (x - s).abs() <= STOP_SLACK)
}

fn same_baseline(a: &Fragment, b: &Fragment) -> bool {
    (a.y - b.y).abs() <= 0.35 * a.size.max(b.size).max(1.0)
}

/// Visual lines of `frags` (top down).
fn visual_lines(frags: Vec<Fragment>) -> Vec<Vec<Fragment>> {
    let mut lines: Vec<Vec<Fragment>> = Vec::new();
    for f in frags {
        match lines.last_mut() {
            Some(l) if same_baseline(&l[0], &f) => l.push(f),
            _ => lines.push(vec![f]),
        }
    }
    for l in &mut lines {
        l.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    lines
}

/// Rebuild the tables among `frags` (one column, top down) as one fragment per row.
pub(crate) fn tables(frags: Vec<Fragment>) -> Vec<Fragment> {
    let lines = visual_lines(frags);
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].len() < 2 {
            out.extend(lines[i].iter().cloned());
            i += 1;
            continue;
        }
        let x0 = lines[i].iter().map(|f| f.x).fold(f32::MAX, f32::min);
        let x1 = lines[i].iter().map(|f| f.end).fold(f32::MIN, f32::max);
        let width = (x1 - x0).max(1.0);
        let labelled = row_label(&lines[i][0].text);
        let size = lines[i].iter().map(|f| f.size).fold(0.0, f32::max);
        let mut j = i + 1;
        while j < lines.len() {
            let l = &lines[j];
            // In a table of row labels, a long line starting where a later cell of the block's
            // lines starts is that cell's text running on (or a name run into its text, with no
            // gap left to part them), not prose.
            let in_cell = labelled
                && lines[i..j]
                    .iter()
                    .flatten()
                    .filter(|f| f.x > lines[i][0].end)
                    .any(|f| (f.x - l[0].x).abs() <= STOP_SLACK);
            let wide = l.len() == 1 && l[0].end - l[0].x > WIDE_LINE * width && !in_cell;
            // A larger line (a heading) ends the table.
            let larger = l.iter().any(|f| f.size > HEADING_RATIO * size);
            let outside = l.iter().any(|f| f.x < x0 - 2.0);
            let far = (lines[j - 1][0].y - l[0].y) > 3.0 * l[0].size.max(1.0);
            if wide || outside || far || larger {
                break;
            }
            j += 1;
        }
        let block: Vec<Fragment> = lines[i..j].iter().flatten().cloned().collect();
        match rows(&block) {
            Some(rows) => out.extend(rows),
            None => out.extend(block),
        }
        i = j;
    }
    out
}

/// One row per baseline of a dense table (most baselines carry [`DENSE_CELLS`] cells or more:
/// one-line rows set at line leading); a lone fragment right of the table's left edge is a
/// wrapped cell and joins the row above.
fn dense_rows(block: &[Fragment]) -> Option<Vec<Fragment>> {
    let lines = visual_lines(block.to_vec());
    let full = lines.iter().filter(|l| l.len() >= DENSE_CELLS).count();
    if (full as f32) < DENSE_SHARE * lines.len() as f32 {
        return None;
    }
    let x0 = block.iter().map(|f| f.x).fold(f32::MAX, f32::min);
    let mut out: Vec<Fragment> = Vec::new();
    for l in lines {
        let text = l
            .iter()
            .map(|f| f.text.trim())
            .collect::<Vec<_>>()
            .join(" | ");
        let wrapped = l.len() == 1 && l[0].x > x0 + l[0].size.max(1.0);
        match out.last_mut() {
            Some(prev) if wrapped => {
                prev.text = format!("{} {text}", prev.text);
            }
            _ => out.push(Fragment {
                x: l[0].x,
                end: l.iter().map(|f| f.end).fold(f32::MIN, f32::max),
                y: l[0].y,
                size: l[0].size,
                bold: l.iter().all(|f| f.bold),
                text,
            }),
        }
    }
    Some(out)
}

/// The rows of a table block, or `None` when it is not a table.
fn rows(block: &[Fragment]) -> Option<Vec<Fragment>> {
    if let Some(dense) = dense_rows(block) {
        return Some(dense);
    }
    // Columns: distinct left edges of multi-fragment lines, clustered.
    let mut edges: Vec<f32> = block.iter().map(|f| f.x).collect();
    edges.sort_by(f32::total_cmp);
    let mut cols: Vec<f32> = Vec::new();
    for e in edges {
        if cols.last().map_or(true, |c| e - c > 3.0 * STOP_SLACK) {
            cols.push(e);
        }
    }
    if cols.len() < 2 {
        return None;
    }
    let col_of = |f: &Fragment| {
        cols.iter()
            .rposition(|c| *c <= f.x + 3.0 * STOP_SLACK)
            .unwrap_or(0)
    };
    // Cells: per column, fragments stacked with line leading.
    let mut cells: Vec<Vec<Vec<&Fragment>>> = vec![Vec::new(); cols.len()];
    for f in block {
        let c = col_of(f);
        match cells[c].last_mut() {
            Some(cell)
                if cell.last().is_some_and(|p| {
                    p.y - f.y <= CELL_LEADING * p.size.max(f.size).max(1.0) && p.y >= f.y
                }) =>
            {
                cell.push(f);
            }
            _ => cells[c].push(vec![f]),
        }
    }
    if let Some(rows) = labelled_rows(block, &cells, &col_of) {
        return Some(rows);
    }
    let anchor = (0..cols.len())
        .max_by_key(|&c| (cells[c].len(), cells[c].iter().map(Vec::len).sum::<usize>()))?;
    if cells[anchor].len() < MIN_ROWS {
        return None;
    }
    let span = |cell: &[&Fragment]| {
        let top = cell.iter().map(|f| f.y + f.size).fold(f32::MIN, f32::max);
        let bottom = cell.iter().map(|f| f.y).fold(f32::MAX, f32::min);
        (top, bottom)
    };
    let row_spans: Vec<(f32, f32)> = cells[anchor].iter().map(|c| span(c)).collect();
    let mut row_cells: Vec<Vec<Vec<String>>> = vec![vec![Vec::new(); cols.len()]; row_spans.len()];
    for (c, col) in cells.iter().enumerate() {
        for cell in col {
            let (top, bottom) = span(cell);
            let mid = (top + bottom) / 2.0;
            let r = (0..row_spans.len())
                .min_by(|&a, &b| {
                    let d = |k: usize| {
                        let (t, b) = row_spans[k];
                        if mid <= t && mid >= b {
                            0.0
                        } else {
                            (mid - t).abs().min((mid - b).abs())
                        }
                    };
                    d(a).total_cmp(&d(b))
                })
                .unwrap_or(0);
            row_cells[r][c].extend(cell.iter().map(|f| f.text.trim().to_string()));
        }
    }
    Some(
        row_cells
            .into_iter()
            .zip(&cells[anchor])
            .map(|(cols_text, anchor_cell)| {
                let text = cols_text
                    .into_iter()
                    .map(|parts| parts.join(" "))
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ");
                let first = anchor_cell[0];
                Fragment {
                    x: block.iter().map(|f| f.x).fold(f32::MAX, f32::min),
                    end: block.iter().map(|f| f.end).fold(f32::MIN, f32::max),
                    y: first.y,
                    size: first.size,
                    bold: anchor_cell.iter().all(|f| f.bold),
                    text,
                }
            })
            .collect(),
    )
}

/// A table row label: a number, a number range or a die ("12", "01-50", "d20").
pub(crate) fn row_label(t: &str) -> bool {
    let t = t.trim();
    let t = t.strip_prefix(['d', 'D']).unwrap_or(t);
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '–' | '+'))
}

/// Rows of a table whose first column is a row label on each row's first baseline: every other
/// fragment belongs to the row of the nearest label at or above it. (A text cell's lines run on
/// at line leading into the next row's, so stacking by leading would merge them; a name cell
/// is centred beside its text.) Lines above the first label (the header) stay rows of their own.
/// Used only when stacking by leading put two labels' rows into one text cell.
fn labelled_rows(
    block: &[Fragment],
    stacked: &[Vec<Vec<&Fragment>>],
    col_of: &dyn Fn(&Fragment) -> usize,
) -> Option<Vec<Fragment>> {
    let columns = stacked.len();
    // The label column: the one with the most row labels; above its first label only the
    // header ("d20") may stand in it or left of it.
    let label_col = (0..columns).max_by_key(|&c| {
        let labels = block
            .iter()
            .filter(|f| col_of(f) == c && row_label(&f.text))
            .count();
        (labels, std::cmp::Reverse(c))
    })?;
    let mut labels: Vec<&Fragment> = block
        .iter()
        .filter(|f| col_of(f) == label_col && row_label(&f.text))
        .collect();
    labels.sort_by(|a, b| b.y.total_cmp(&a.y));
    let first_y = labels.first()?.y;
    let stray = block.iter().any(|f| {
        col_of(f) <= label_col && !row_label(&f.text) && f.y <= first_y + 0.35 * f.size.max(1.0)
    });
    let on_a_row = |l: &&Fragment| {
        block
            .iter()
            .any(|f| col_of(f) > label_col && same_baseline(l, f))
    };
    if labels.len() < MIN_ROWS || stray || !labels.iter().all(on_a_row) {
        return None;
    }
    let merged = (0..columns)
        .filter(|&c| c != label_col)
        .flat_map(|c| &stacked[c])
        .any(|cell| {
            labels
                .iter()
                .filter(|l| cell.iter().any(|f| same_baseline(l, f)))
                .count()
                >= 2
        });
    if !merged {
        return None;
    }
    let slack = |f: &Fragment| 0.35 * f.size.max(1.0);
    let mut header: Vec<Fragment> = Vec::new();
    let mut cells: Vec<Vec<Vec<&Fragment>>> = vec![vec![Vec::new(); columns]; labels.len()];
    let mut ordered: Vec<&Fragment> = block.iter().collect();
    ordered.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    for f in ordered {
        match labels.iter().rposition(|l| l.y + slack(l) >= f.y) {
            Some(r) => cells[r][col_of(f)].push(f),
            None => header.push(f.clone()),
        }
    }
    let x = block.iter().map(|f| f.x).fold(f32::MAX, f32::min);
    let end = block.iter().map(|f| f.end).fold(f32::MIN, f32::max);
    let mut out: Vec<Fragment> = visual_lines(header)
        .into_iter()
        .map(|l| Fragment {
            text: l
                .iter()
                .map(|f| f.text.trim())
                .collect::<Vec<_>>()
                .join(" | "),
            x: l[0].x,
            end: l.iter().map(|f| f.end).fold(f32::MIN, f32::max),
            y: l[0].y,
            size: l[0].size,
            bold: l.iter().all(|f| f.bold),
        })
        .collect();
    for (label, row) in labels.iter().zip(cells) {
        let text = row
            .iter()
            .map(|cell| {
                cell.iter()
                    .map(|f| f.text.trim())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        out.push(Fragment {
            x,
            end,
            y: label.y,
            size: label.size,
            bold: false,
            text,
        });
    }
    Some(out)
}

// ---- tables across a column gutter ------------------------------------------------------------

/// Share of the lines on each side of a band that must share a baseline with a line on the
/// other side for the two sides to be one table's columns.
const PAIRED_SHARE: f32 = 0.8;
/// Lines smaller than this share of the band's usual size (marks, superscripts) are left out.
const MARK_SIZE: f32 = 0.75;

/// The two sides of a band pair up line by line on shared baselines (one line a side per
/// baseline, at least two lines a side, nearly all with a partner on the other side): a
/// two-column table, not two columns.
pub(crate) fn paired_rows(left: &[Fragment], right: &[Fragment]) -> bool {
    let mut sizes: Vec<f32> = left.iter().chain(right).map(|f| f.size).collect();
    sizes.sort_by(f32::total_cmp);
    let Some(usual) = sizes.get(sizes.len() / 2).copied() else {
        return false;
    };
    let lines = |side: &[Fragment]| -> Vec<Fragment> {
        side.iter()
            .filter(|f| f.size >= MARK_SIZE * usual)
            .cloned()
            .collect()
    };
    let (l, r) = (lines(left), lines(right));
    let partnered = |a: &[Fragment], b: &[Fragment]| {
        let n = a
            .iter()
            .filter(|f| b.iter().any(|g| same_baseline(f, g)))
            .count();
        a.len() >= 2 && n as f32 >= PAIRED_SHARE * a.len() as f32
    };
    // One cell a side per row: a side with two cells on a baseline is itself columns.
    let single = |a: &[Fragment]| {
        a.iter()
            .enumerate()
            .all(|(i, f)| !a[i + 1..].iter().any(|g| same_baseline(f, g)))
    };
    single(&l) && single(&r) && partnered(&l, &r) && partnered(&r, &l)
}

/// Which side of a gutter a fragment is on (`None`: it crosses the gutter).
pub(crate) type SideOf<'a> = &'a dyn Fn(&Fragment) -> Option<bool>;

/// Fewest rows with cells on both sides of a gutter that make a table spanning it.
const SPAN_MIN_ROWS: usize = 3;
/// Most font sizes between two such rows of one table (a wrapped cell lies between them).
const SPAN_ROW_GAP: f32 = 4.0;
/// Cells on one baseline across a gutter that make it a table row (two could be two columns'
/// short lines, such as lists side by side).
const SPAN_MIN_CELLS: usize = 3;
/// A line starting within this many font sizes of a column's usual left edge continues it.
const EDGE_SLACK: f32 = 1.0;

/// Fragments of one baseline, top down (fragments come top down).
fn baseline_rows(frags: &[Fragment]) -> Vec<Vec<&Fragment>> {
    let mut rows: Vec<Vec<&Fragment>> = Vec::new();
    for f in frags {
        match rows.last_mut() {
            Some(r) if same_baseline(r[0], f) => r.push(f),
            _ => rows.push(vec![f]),
        }
    }
    rows
}

/// A table across a column gutter: its first and last baselines, and whether it opens with a
/// header row and has one side of one cell a row (read by [`headed::rows`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    pub top: f32,
    pub bottom: f32,
    pub headed: bool,
}

/// The y ranges (top, bottom baseline) of tables whose columns lie on both sides of a gutter:
/// at least [`SPAN_MIN_ROWS`] close rows of [`SPAN_MIN_CELLS`] cells or more on both sides,
/// unless one side is a column of prose going on above or below them at the same left edge (a
/// table in the other column), both sides start at their columns' margins (a table in each
/// column) or both sides have one format (a table continued in the next column). Such a table
/// is read row by row, not split into columns. Rows whose gap is filled by the lines of a
/// wrapped cell stay one table however long the cell, and a table that opens with a header row
/// (every cell bold) over a side of one cell a row is a table even where that side starts at
/// its column's margin like prose ([`headed`]).
pub(crate) fn spanning_tables(frags: &[Fragment], side: SideOf) -> Vec<Span> {
    let rows = baseline_rows(frags);
    let qualifies = |r: &[&Fragment]| {
        let l = r.iter().filter(|f| side(f) == Some(true)).count();
        let n = r.iter().filter(|f| side(f) == Some(false)).count();
        !r.iter().any(|f| side(f).is_none()) && l > 0 && n > 0 && l + n >= SPAN_MIN_CELLS
    };
    let mut out = Vec::new();
    let mut run: Vec<&[&Fragment]> = Vec::new();
    for r in rows.iter().filter(|r| qualifies(r)) {
        let close = run.last().is_some_and(|p| {
            let size = p[0].size.max(1.0);
            p[0].y - r[0].y <= SPAN_ROW_GAP * size || headed::filled(frags, p[0].y, r[0].y, size)
        });
        if !close {
            push_span(&mut out, &run, frags, side);
            run.clear();
        }
        run.push(r);
    }
    push_span(&mut out, &run, frags, side);
    out
}

/// A line this much larger than the rows' text heads a list.
const LIST_HEAD_RATIO: f32 = 1.12;
/// Font sizes above or below a list heading where the next list's lines pass beside it.
const LIST_HEAD_REACH: f32 = 1.5;

/// Lists set side by side, not a table: below the first row, larger lines (list headings such
/// as "Level 2", with another list's lines passing beside them) start at two or more of the
/// rows' cell edges, in the rows or in the lines that go on under them at those edges. A
/// table's own heading rows stand alone on their band.
fn headed_lists(all: &[Fragment], run: &[&[&Fragment]]) -> bool {
    let (Some(first), Some(last)) = (run.first(), run.last()) else {
        return false;
    };
    let top = first[0].y;
    let mut cell_edges: Vec<f32> = Vec::new();
    for f in run.iter().flat_map(|r| r.iter()) {
        if !cell_edges
            .iter()
            .any(|e| (e - f.x).abs() <= EDGE_SLACK * f.size)
        {
            cell_edges.push(f.x);
        }
    }
    let at_edge = |f: &Fragment| {
        cell_edges
            .iter()
            .any(|e| (e - f.x).abs() <= EDGE_SLACK * f.size.max(1.0))
    };
    // The lines at the cell edges from the first row down, while they keep close together.
    let mut bottom = last[0].y;
    let mut lines: Vec<&Fragment> = Vec::new();
    for f in all
        .iter()
        .filter(|f| at_edge(f) && f.y <= top + 0.35 * f.size)
    {
        if f.y < bottom - SPAN_ROW_GAP * f.size.max(1.0) {
            break;
        }
        bottom = bottom.min(f.y);
        lines.push(f);
    }
    let mut sizes: Vec<f32> = lines.iter().map(|f| f.size).collect();
    sizes.sort_by(f32::total_cmp);
    let Some(body) = sizes.get(sizes.len() / 2) else {
        return false;
    };
    let small = |f: &Fragment| f.size < LIST_HEAD_RATIO * body;
    // A list heading has another list's line beside it; a table's heading row stands alone.
    let beside = |h: &Fragment| {
        lines.iter().any(|g| {
            small(g)
                && (g.x - h.x).abs() > EDGE_SLACK * g.size.max(1.0)
                && (g.y - h.y).abs() <= LIST_HEAD_REACH * h.size
        })
    };
    let mut edges: Vec<f32> = Vec::new();
    for f in &lines {
        let first_row = (f.y - top).abs() <= 0.35 * f.size.max(1.0);
        if first_row || small(f) || !beside(f) {
            continue;
        }
        if !edges.iter().any(|e| (e - f.x).abs() <= EDGE_SLACK * f.size) {
            edges.push(f.x);
        }
    }
    edges.len() >= 2
}

fn push_span(out: &mut Vec<Span>, run: &[&[&Fragment]], all: &[Fragment], side: SideOf) {
    let (Some(first), Some(last)) = (run.first(), run.last()) else {
        return;
    };
    let (top, bottom) = (first[0].y, last[0].y);
    if run.len() < SPAN_MIN_ROWS || headed_lists(all, run) || side_by_side(all, top, bottom, side) {
        return;
    }
    // A header row over a side of single cells: that side is the table's last column, not a
    // column of prose, though it starts at the column's margin.
    let headed = headed::is_headed(run, all, top, bottom, side);
    if !headed
        && (continues_column(all, top, bottom, true, side)
            || continues_column(all, top, bottom, false, side)
            || (column_edge(all, top, bottom, true, side).is_some_and(|at| at)
                && column_edge(all, top, bottom, false, side).is_some_and(|at| at)))
    {
        return;
    }
    let bottom = if headed {
        headed::wrapped_bottom(all, run, bottom)
    } else {
        bottom
    };
    out.push(Span {
        top,
        bottom,
        headed,
    });
}

fn within(f: &Fragment, top: f32, bottom: f32) -> bool {
    let slack = 0.35 * f.size.max(1.0);
    f.y <= top + slack && f.y >= bottom - slack
}

/// The usual left edge of the `left` side's lines outside `top`..`bottom` (a column's margin),
/// with the lines inside and outside; `None` with fewer than three lines outside.
fn margin<'a>(
    all: &'a [Fragment],
    top: f32,
    bottom: f32,
    left: bool,
    side: SideOf,
) -> Option<(f32, Vec<&'a Fragment>)> {
    let (inside, outside): (Vec<&Fragment>, Vec<&Fragment>) = all
        .iter()
        .filter(|f| side(f) == Some(left))
        .partition(|f| within(f, top, bottom));
    if outside.len() < 3 || inside.is_empty() {
        return None;
    }
    let near = |a: f32, b: &Fragment| (a - b.x).abs() <= EDGE_SLACK * b.size.max(1.0);
    let edge = outside
        .iter()
        .map(|f| f.x)
        .max_by_key(|x| outside.iter().filter(|g| near(*x, g)).count())?;
    Some((edge, inside))
}

/// Whether the `left` side's part of the rows starts at that side's column margin (a table
/// set inside its column); `None` when the side has no column around the rows.
fn column_edge(all: &[Fragment], top: f32, bottom: f32, left: bool, side: SideOf) -> Option<bool> {
    let (edge, inside) = margin(all, top, bottom, left, side)?;
    let first = inside.iter().map(|f| f.x).fold(f32::MAX, f32::min);
    let size = inside.iter().map(|f| f.size).fold(1.0, f32::max);
    Some((first - edge).abs() <= EDGE_SLACK * size)
}

/// The `left` side between `top` and `bottom` is a column of prose: one line per baseline,
/// starting where that side's lines above and below start.
fn continues_column(all: &[Fragment], top: f32, bottom: f32, left: bool, side: SideOf) -> bool {
    let Some((edge, inside)) = margin(all, top, bottom, left, side) else {
        return false;
    };
    let one_per_row = baseline_rows(&inside.iter().map(|f| (*f).clone()).collect::<Vec<_>>())
        .iter()
        .all(|r| r.len() == 1);
    let at_edge = inside
        .iter()
        .filter(|f| (edge - f.x).abs() <= EDGE_SLACK * f.size.max(1.0))
        .count();
    one_per_row && 2 * at_edge >= inside.len()
}

/// Left edges (relative to the side's leftmost) recurring on at least two baselines of `frags`.
fn edges(frags: &[&Fragment]) -> Vec<f32> {
    let x0 = frags.iter().map(|f| f.x).fold(f32::MAX, f32::min);
    let mut xs: Vec<f32> = frags.iter().map(|f| f.x - x0).collect();
    xs.sort_by(f32::total_cmp);
    let mut out: Vec<(f32, usize)> = Vec::new();
    for x in xs {
        match out.last_mut() {
            Some((e, n)) if x - *e <= 3.0 * STOP_SLACK => *n += 1,
            _ => out.push((x, 1)),
        }
    }
    out.into_iter()
        .filter(|(_, n)| *n >= 2)
        .map(|(e, _)| e)
        .collect()
}

/// The `left` side's lines between `top` and `bottom`, with that side's table rows (two or more
/// cells on one baseline) above and below them: the layout of a table of the side's own, whose
/// rows start on other baselines than the next column's (a column of labels may have no line
/// between `top` and `bottom`).
fn side_layout(all: &[Fragment], top: f32, bottom: f32, left: bool, side: SideOf) -> Vec<Fragment> {
    let mine: Vec<Fragment> = all
        .iter()
        .filter(|f| side(f) == Some(left))
        .cloned()
        .collect();
    baseline_rows(&mine)
        .into_iter()
        .filter(|row| row.len() >= 2 || row.iter().all(|f| within(f, top, bottom)))
        .flatten()
        .cloned()
        .collect()
}

/// The two sides between `top` and `bottom` are tables of one format next to each other (a
/// long table continued in the next column, or one table in each column whose rows happen to
/// share baselines): each side has a row of two or more cells, and one side's column edges
/// (from its table rows around the band too) recur, shifted, as a run of the other side's.
fn side_by_side(all: &[Fragment], top: f32, bottom: f32, side: SideOf) -> bool {
    let part = |left: bool| -> Vec<&Fragment> {
        all.iter()
            .filter(|f| side(f) == Some(left) && within(f, top, bottom))
            .collect()
    };
    let (l, r) = (part(true), part(false));
    let size = l.iter().chain(&r).map(|f| f.size).fold(1.0, f32::max);
    // A continued table has two or more cells on some row of each side.
    let cells = |side: &[&Fragment]| {
        let owned: Vec<Fragment> = side.iter().map(|f| (*f).clone()).collect();
        baseline_rows(&owned)
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(0)
    };
    if cells(&l).min(cells(&r)) < 2 {
        return false;
    }
    let (wide_l, wide_r) = (
        side_layout(all, top, bottom, true, side),
        side_layout(all, top, bottom, false, side),
    );
    let (el, er) = (
        edges(&wide_l.iter().collect::<Vec<_>>()),
        edges(&wide_r.iter().collect::<Vec<_>>()),
    );
    let (short, long) = if el.len() <= er.len() {
        (&el, &er)
    } else {
        (&er, &el)
    };
    short.len() >= 2
        && long.windows(short.len()).any(|w| {
            w.iter()
                .zip(short.iter())
                .all(|(a, b)| ((a - w[0]) - (b - short[0])).abs() <= 1.5 * size)
        })
}
