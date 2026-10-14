//! Stacked blocks on a page of columns: a block heading much larger than the body text, alone
//! on its band, starts a block that is read across all its columns before the next block (a
//! list page that stacks one class's four list columns over the next class's).

use super::Fragment;

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
pub(crate) fn block_cut(
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
