//! Headed tables across a column gutter: a bold header row over rows whose last column holds
//! running text (a property, an effect) that wraps over several lines. Read column by column,
//! each column's cells would land under the column's header as if it were a heading, and every
//! row's name would stand alone; here a row starts at each line with a first-column cell, and
//! every line below it (a wrapped cell) joins the cell of its column.

use super::super::pdf::Fragment;
use super::{baseline_rows, visual_lines, within, SideOf, STOP_SLACK};

/// Most font sizes between two lines of one table (a row's wrapped lines, the next row).
const WRAP_GAP: f32 = 1.6;
/// Cells on a header row (as on a table row across a gutter).
const HEADER_CELLS: usize = 3;
/// Points within which left edges are one column edge.
const EDGE_JOIN: f32 = 3.0 * STOP_SLACK;

/// Every gap between the baselines from `top` down to `bottom` is at most [`WRAP_GAP`] font
/// sizes: the lines of wrapped cells fill the space between two rows.
pub(crate) fn filled(frags: &[Fragment], top: f32, bottom: f32, size: f32) -> bool {
    let slack = 0.35 * size;
    let mut ys: Vec<f32> = frags
        .iter()
        .filter(|f| f.y <= top + slack && f.y >= bottom - slack)
        .map(|f| f.y)
        .collect();
    ys.sort_by(|a, b| b.total_cmp(a));
    ys.windows(2).all(|w| w[0] - w[1] <= WRAP_GAP * size)
}

/// The run of rows opens with a header row (every cell bold, on both sides of the gutter), not
/// every row is bold, and one side holds one cell a baseline between `top` and `bottom`.
pub(crate) fn is_headed(
    run: &[&[&Fragment]],
    all: &[Fragment],
    top: f32,
    bottom: f32,
    side: SideOf,
) -> bool {
    let Some(head) = run.first() else {
        return false;
    };
    let bold_row = |r: &[&Fragment]| r.iter().all(|f| f.bold);
    if head.len() < HEADER_CELLS || !bold_row(head) || run.iter().all(|r| bold_row(r)) {
        return false;
    }
    let single = |left: bool| {
        let part: Vec<Fragment> = all
            .iter()
            .filter(|f| side(f) == Some(left) && within(f, top, bottom))
            .cloned()
            .collect();
        !part.is_empty() && baseline_rows(&part).iter().all(|r| r.len() == 1)
    };
    single(true) || single(false)
}

/// The last baseline of the table: below its last row, lines that keep the table's leading and
/// start at a column edge other than the first (the last row's wrapped cells) belong to it.
pub(crate) fn wrapped_bottom(all: &[Fragment], run: &[&[&Fragment]], bottom: f32) -> f32 {
    let first = run
        .iter()
        .flat_map(|r| r.iter())
        .map(|f| f.x)
        .fold(f32::MAX, f32::min);
    let edges: Vec<f32> = run
        .iter()
        .flat_map(|r| r.iter())
        .map(|f| f.x)
        .filter(|x| *x > first + EDGE_JOIN)
        .collect();
    let at_edge = |f: &Fragment| edges.iter().any(|e| (e - f.x).abs() <= EDGE_JOIN);
    let below: Vec<Fragment> = all
        .iter()
        .filter(|f| f.y < bottom - 0.35 * f.size.max(1.0))
        .cloned()
        .collect();
    let mut last = bottom;
    for row in baseline_rows(&below) {
        let size = row[0].size.max(1.0);
        if last - row[0].y > WRAP_GAP * size || !row.iter().all(|f| at_edge(f)) {
            break;
        }
        last = row[0].y;
    }
    last
}

/// A row being built: its first line and its cells by column edge.
struct Row {
    first: Fragment,
    end: f32,
    lead: String,
    cells: Vec<Vec<String>>,
}

/// `parts` (a cell's lines) as one text; a line ending in a hyphen runs on into the next.
fn join_parts(parts: &[String]) -> String {
    let mut out = String::new();
    for p in parts {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let glued = out.ends_with('-') && p.starts_with(|c: char| c.is_lowercase());
        if !out.is_empty() && !glued {
            out.push(' ');
        }
        out.push_str(p);
    }
    out
}

/// A first-column line that goes on with the cell above it (a wrapped name).
fn runs_on(lead: &str, text: &str) -> bool {
    text.trim_start().starts_with(|c: char| c.is_lowercase())
        || lead.trim_end().ends_with(['-', ','])
}

/// The rows of a headed table (fragments top down): a row starts at each line with a cell left
/// of the table's second column, and the lines below it join their columns' cells.
pub(crate) fn rows(frags: Vec<Fragment>) -> Vec<Fragment> {
    let lines = visual_lines(frags);
    let second = lines
        .iter()
        .filter(|l| l.len() >= HEADER_CELLS)
        .map(|l| l[1].x)
        .fold(f32::MAX, f32::min);
    let mut edges: Vec<f32> = lines.iter().flatten().map(|f| f.x).collect();
    edges.sort_by(f32::total_cmp);
    edges.dedup_by(|b, a| *b - *a <= EDGE_JOIN);
    let column = |x: f32| edges.iter().rposition(|e| *e <= x + EDGE_JOIN).unwrap_or(0);
    let mut rows: Vec<Row> = Vec::new();
    for line in lines {
        let lead = line.iter().find(|f| f.x < second - STOP_SLACK);
        let starts = match (lead, rows.last()) {
            (Some(f), Some(r)) => !runs_on(&r.lead, &f.text),
            (_, None) => true,
            (None, Some(_)) => false,
        };
        if starts {
            rows.push(Row {
                first: line[0].clone(),
                end: line[0].end,
                lead: String::new(),
                cells: vec![Vec::new(); edges.len()],
            });
        }
        let Some(row) = rows.last_mut() else {
            continue;
        };
        if let Some(f) = lead {
            row.lead.clone_from(&f.text);
        }
        for f in &line {
            row.end = row.end.max(f.end);
            row.cells[column(f.x)].push(f.text.trim().to_string());
        }
    }
    rows.into_iter()
        .map(|r| {
            let text = r
                .cells
                .iter()
                .map(|c| join_parts(c))
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" | ");
            Fragment {
                end: r.end,
                text,
                ..r.first
            }
        })
        .collect()
}
