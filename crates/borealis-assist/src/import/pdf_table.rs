//! Tables on a PDF page: cells are told apart by shared left edges (tab stops), stacked into
//! cells, and the cells of one row (vertically centred or not) are joined into one
//! `cell | cell | cell` line, so a row's item name stays whole and next to its text.

use super::pdf::Fragment;
use super::pdf_walk::Run;

/// A run starting this many points from a stop starts at it.
const STOP_SLACK: f32 = 1.0;
/// Baselines on which a left edge must recur to be a tab stop.
const MIN_STOP_LINES: usize = 4;
/// The gap before a cell is at least this many font sizes (wider than a word space).
pub(crate) const CELL_GAP: f32 = 0.3;
/// Lines of one cell are at most this many font sizes apart.
const CELL_LEADING: f32 = 1.45;
/// A single fragment wider than this share of the table ends it (a prose line).
const WIDE_LINE: f32 = 0.6;
/// Fewest rows a table has.
const MIN_ROWS: usize = 2;

/// Left edges where runs start on at least [`MIN_STOP_LINES`] baselines (runs sorted top down).
pub(crate) fn tab_stops(runs: &[Run]) -> Vec<f32> {
    let mut starts: Vec<(f32, f32)> = runs.iter().map(|r| (r.x, r.y)).collect();
    starts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut stops = Vec::new();
    let mut i = 0;
    while i < starts.len() {
        let mut j = i;
        let mut ys: Vec<f32> = Vec::new();
        while j < starts.len() && starts[j].0 - starts[i].0 <= STOP_SLACK {
            if !ys.iter().any(|y| (y - starts[j].1).abs() < 1.0) {
                ys.push(starts[j].1);
            }
            j += 1;
        }
        if ys.len() >= MIN_STOP_LINES {
            stops.push(starts[i].0);
        }
        i = j;
    }
    stops
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
        let mut j = i + 1;
        while j < lines.len() {
            let l = &lines[j];
            let wide = l.len() == 1 && l[0].end - l[0].x > WIDE_LINE * width;
            let outside = l.iter().any(|f| f.x < x0 - 2.0);
            let far = (lines[j - 1][0].y - l[0].y) > 3.0 * l[0].size.max(1.0);
            if wide || outside || far {
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

/// The rows of a table block, or `None` when it is not a table.
fn rows(block: &[Fragment]) -> Option<Vec<Fragment>> {
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
