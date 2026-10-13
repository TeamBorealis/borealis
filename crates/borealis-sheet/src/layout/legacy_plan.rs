//! Row plan of the basic ("legacy") spellcasting page: how many name rows each spell level gets
//! and which levels share a column.
//!
//! The page keeps the fixed capacities of the basic spell page (cantrips 8, then
//! 12, 13, 13, 13, 9, 9, 9, 7, 7, in the columns 0-2 / 3-5 / 6-9) whenever every level fits. When a
//! level has more spells than its rows, the blank rows of other levels move to it (highest levels
//! first, each keeping at least [`MIN_ROWS`] write-in rows), and the levels are regrouped into the
//! three columns that give the tallest rows. A level too long for one column flows on into the
//! next under a "Level N (cont.)" header. Spells are only left out (and counted for the "+N more"
//! note) when even rows of [`MIN_ROW_H`] cannot hold them all.

use crate::model_types::{CasterData, SpellRow};

/// Name rows per spell level on the basic page: cantrips, then levels 1 to 9.
pub(crate) const ROWS: [usize; 10] = [8, 12, 13, 13, 13, 9, 9, 9, 7, 7];
/// Spell levels per column.
pub(crate) const COLUMNS: [&[usize]; 3] = [&[0, 1, 2], &[3, 4, 5], &[6, 7, 8, 9]];
/// Height of a level header plus the gap above it.
pub(crate) const LEVEL_COST: f32 = LEVEL_H + LEVEL_GAP;
pub(crate) const LEVEL_H: f32 = 13.0;
/// Space above every level header but the first of a column.
pub(crate) const LEVEL_GAP: f32 = 4.0;
/// Tallest row.
pub(crate) const MAX_ROW_H: f32 = 14.5;
/// Shortest row: the 7.2 pt name still fits its field with the pip beside it.
pub(crate) const MIN_ROW_H: f32 = 10.0;
/// Write-in rows a level keeps when it gives its blank rows away.
pub(crate) const MIN_ROWS: usize = 2;

/// A run of rows of one level in one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Seg {
    pub level: usize,
    /// Index of the first row (0-based) of the level in this run.
    pub first: usize,
    pub count: usize,
    /// A later run of a level split between columns.
    pub cont: bool,
}

/// Rows and columns of one caster's basic page.
#[derive(Debug)]
pub(crate) struct LegacyPlan<'a> {
    /// The spells printed per level (at most `rows[level]`).
    pub by: [Vec<&'a SpellRow>; 10],
    /// Level runs per column, in order.
    pub columns: Vec<Vec<Seg>>,
    /// Row height for a column body of `avail` points.
    pub row_h: f32,
    /// Spells left out (only when the page cannot hold them at [`MIN_ROW_H`]).
    pub dropped: usize,
}

/// Row height of `columns` in `avail` points (the fullest column decides; capped at
/// [`MAX_ROW_H`]).
pub(crate) fn row_height(rows: &[usize; 10], columns: &[Vec<usize>], avail: f32) -> f32 {
    columns
        .iter()
        .map(|levels| {
            let n: usize = levels.iter().map(|l| rows[*l]).sum();
            let room = avail - LEVEL_COST * levels.len() as f32;
            if n == 0 {
                MAX_ROW_H
            } else {
                room / n as f32
            }
        })
        .fold(MAX_ROW_H, f32::min)
}

fn default_columns() -> Vec<Vec<usize>> {
    COLUMNS.iter().map(|c| c.to_vec()).collect()
}

/// The three-column split of levels 0..=9 (in order) with the tallest rows; the spec split wins
/// ties.
fn best_columns(rows: &[usize; 10], avail: f32) -> (Vec<Vec<usize>>, f32) {
    let mut best = default_columns();
    let mut best_h = row_height(rows, &best, avail);
    for a in 1..9 {
        for b in a + 1..10 {
            let cols = vec![(0..a).collect(), (a..b).collect(), (b..10).collect()];
            let h = row_height(rows, &cols, avail);
            if h > best_h + 0.01 {
                best = cols;
                best_h = h;
            }
        }
    }
    (best, best_h)
}

/// Give blank rows of other levels (highest first) to levels with more spells than rows.
fn redistribute(need: &[usize; 10]) -> [usize; 10] {
    let mut rows = ROWS;
    for (r, n) in rows.iter_mut().zip(need) {
        *r = (*r).max(*n);
    }
    let budget: usize = ROWS.iter().sum();
    let mut excess = rows.iter().sum::<usize>().saturating_sub(budget);
    for l in (0..10).rev() {
        let floor = need[l].max(MIN_ROWS);
        let give = rows[l].saturating_sub(floor).min(excess);
        rows[l] -= give;
        excess -= give;
    }
    rows
}

/// Smallest run of a split level, in rows.
const MIN_RUN: usize = 2;
/// Step when looking for the tallest rows of a flowed page.
const ROW_STEP: f32 = 0.25;

/// Whole levels as runs.
fn whole(rows: &[usize; 10], columns: &[Vec<usize>]) -> Vec<Vec<Seg>> {
    columns
        .iter()
        .map(|levels| {
            levels
                .iter()
                .map(|&level| Seg {
                    level,
                    first: 0,
                    count: rows[level],
                    cont: false,
                })
                .collect()
        })
        .collect()
}

/// Rows (of `left` still to place) that a column with `used` points taken can hold.
fn run_len(left: usize, used: f32, gap: f32, avail: f32, row_h: f32) -> Option<usize> {
    let room = avail - used - gap - LEVEL_H;
    if room < 0.0 {
        return None;
    }
    let n = ((room / row_h) + 1e-3).floor() as usize;
    (n >= left || n >= MIN_RUN).then_some(n.min(left))
}

/// Pour the levels through three columns at `row_h`, splitting a level between columns. `None`
/// when they do not fit.
fn flow(rows: &[usize; 10], avail: f32, row_h: f32) -> Option<Vec<Vec<Seg>>> {
    let mut cols: Vec<Vec<Seg>> = vec![Vec::new()];
    let mut used = 0.0;
    for (level, &total) in rows.iter().enumerate() {
        let mut first = 0;
        loop {
            let col = cols.last_mut()?;
            let gap = if col.is_empty() { 0.0 } else { LEVEL_GAP };
            if let Some(n) = run_len(total - first, used, gap, avail, row_h) {
                col.push(Seg {
                    level,
                    first,
                    count: n,
                    cont: first > 0,
                });
                used += gap + LEVEL_H + n as f32 * row_h;
                first += n;
                if first == total {
                    break;
                }
            }
            if cols.len() == 3 {
                return None;
            }
            cols.push(Vec::new());
            used = 0.0;
        }
    }
    Some(cols)
}

/// The runs and row height of `rows`: whole levels in the best three columns when their rows
/// stay at least [`MIN_ROW_H`] tall, else the tallest flowed layout. `None` when nothing fits.
fn arrange(rows: &[usize; 10], avail: f32) -> Option<(Vec<Vec<Seg>>, f32)> {
    let (cols, h) = best_columns(rows, avail);
    if h >= MIN_ROW_H {
        return Some((whole(rows, &cols), h.min(MAX_ROW_H)));
    }
    let mut row_h = MAX_ROW_H;
    while row_h >= MIN_ROW_H {
        if let Some(cols) = flow(rows, avail, row_h) {
            return Some((cols, row_h));
        }
        row_h -= ROW_STEP;
    }
    None
}

/// Take rows away until the page holds them at [`MIN_ROW_H`]: blank rows first (from the level
/// with the most), then spells (from the longest level). Returns the spells left out.
fn shrink_to_fit(rows: &mut [usize; 10], need: &[usize; 10], avail: f32) -> usize {
    let mut dropped = 0;
    while flow(rows, avail, MIN_ROW_H).is_none() {
        let spare = (0..10).max_by_key(|&l| (rows[l].saturating_sub(need[l]), rows[l]));
        let Some(l) = spare.filter(|&l| rows[l] > 0) else {
            break;
        };
        if rows[l] <= need[l] {
            dropped += 1;
        }
        rows[l] -= 1;
    }
    dropped
}

/// Plan the basic page of `c` for a column body `avail` points tall.
pub(crate) fn plan(c: &CasterData, include_unprepared: bool, avail: f32) -> LegacyPlan<'_> {
    let mut by: [Vec<&SpellRow>; 10] = Default::default();
    for r in c.cantrips.iter().chain(c.listed(include_unprepared)) {
        if let Some(v) = by.get_mut(r.level as usize) {
            v.push(r);
        }
    }
    let need: [usize; 10] = std::array::from_fn(|l| by[l].len());
    if need.iter().zip(ROWS).all(|(n, r)| *n <= r) {
        let columns = default_columns();
        let row_h = row_height(&ROWS, &columns, avail);
        return LegacyPlan {
            by,
            columns: whole(&ROWS, &columns),
            row_h,
            dropped: 0,
        };
    }
    let mut rows = redistribute(&need);
    let dropped = shrink_to_fit(&mut rows, &need, avail);
    for (v, r) in by.iter_mut().zip(rows) {
        v.truncate(r);
    }
    let (columns, row_h) = arrange(&rows, avail).unwrap_or_else(|| {
        let cols = default_columns();
        let h = row_height(&rows, &cols, avail);
        (whole(&rows, &cols), h)
    });
    LegacyPlan {
        by,
        columns,
        row_h,
        dropped,
    }
}

/// Header of a run: "Cantrips", "Level 3", "Level 3 (cont.)".
pub(crate) fn level_title(seg: Seg) -> String {
    let base = if seg.level == 0 {
        "Cantrips".to_string()
    } else {
        format!("Level {}", seg.level)
    };
    if seg.cont {
        format!("{base} (cont.)")
    } else {
        base
    }
}

/// The "+N more spells not shown" note (empty when nothing was left out).
pub(crate) fn dropped_note(dropped: usize) -> String {
    match dropped {
        0 => String::new(),
        1 => "+1 more spell not shown (the basic page is full)".to_string(),
        n => format!("+{n} more spells not shown (the basic page is full)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Column body of a Letter page (the PDF value).
    const AVAIL: f32 = 573.0;

    #[test]
    fn spec_capacities_stay_when_every_level_fits() {
        let rows = redistribute(&[8, 12, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(rows, ROWS);
    }

    #[test]
    fn an_overfull_level_takes_blank_rows_from_the_highest_levels() {
        let rows = redistribute(&[3, 14, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(rows[1], 14);
        assert_eq!(rows.iter().sum::<usize>(), ROWS.iter().sum::<usize>());
        assert_eq!(rows[9], 5, "level 9 gives its two blank rows first");
        assert_eq!(&rows[..9], &[8, 14, 13, 13, 13, 9, 9, 9, 7]);
    }

    #[test]
    fn many_spells_regroup_the_columns_and_drop_nothing() {
        let need = [10, 40, 20, 5, 0, 0, 0, 0, 0, 0];
        let mut rows = redistribute(&need);
        assert_eq!(shrink_to_fit(&mut rows, &need, AVAIL), 0);
        let (cols, h) = arrange(&rows, AVAIL).expect("fits");
        assert!(h >= MIN_ROW_H, "row height {h}");
        let levels: Vec<usize> = cols.concat().iter().map(|s| s.level).collect();
        assert_eq!(levels, (0..10).collect::<Vec<_>>());
        for (r, n) in rows.iter().zip(need) {
            assert!(*r >= n);
        }
    }

    #[test]
    fn only_an_impossible_list_drops_spells_and_counts_them() {
        let need = [30, 200, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut rows = redistribute(&need);
        let dropped = shrink_to_fit(&mut rows, &need, AVAIL);
        assert!(dropped > 0);
        let shown: usize = rows.iter().zip(need).map(|(r, n)| (*r).min(n)).sum();
        assert_eq!(shown + dropped, 230);
        assert!(arrange(&rows, AVAIL).is_some());
    }

    #[test]
    fn a_level_longer_than_a_column_flows_on_under_a_cont_header() {
        let need = [10, 70, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut rows = redistribute(&need);
        assert_eq!(shrink_to_fit(&mut rows, &need, AVAIL), 0);
        let (cols, h) = arrange(&rows, AVAIL).expect("fits");
        assert!(h >= MIN_ROW_H);
        let ones: Vec<Seg> = cols.concat().into_iter().filter(|s| s.level == 1).collect();
        assert!(ones.len() >= 2, "{ones:?}");
        assert!(!ones[0].cont && ones[1..].iter().all(|s| s.cont));
        let mut next = 0;
        for s in &ones {
            assert_eq!(s.first, next, "runs are contiguous");
            next += s.count;
        }
        assert_eq!(next, 70);
    }
}
