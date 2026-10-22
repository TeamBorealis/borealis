//! Companion page (S6): the companion creature's stat block, poured into the same two ruled
//! columns as the Story page. Printed only when the app supplied a stat block
//! (`SheetData::extras.companion`).

use crate::layout::grid::Grid;
use crate::layout::page_features::features_flow::{fill, Cx, Piece, Region};
use crate::layout::page_features::features_pages::{columns, new_canvas, Env, Seq};
use crate::layout::LayoutOptions;
use crate::model_types::SheetData;
use crate::pdf::PdfPage;
use std::collections::VecDeque;

/// Smallest ruled area of a section, in lines.
const MIN_LINES: usize = 2;

/// The statistics and skills go in the left column, the creature's traits, actions and
/// reactions in the right one.
fn queues(data: &SheetData) -> (VecDeque<Piece>, VecDeque<Piece>) {
    let mut left = VecDeque::new();
    let mut right = VecDeque::new();
    let Some(companion) = &data.extras.companion else {
        return (left, right);
    };
    for (id, title, text) in companion.regions() {
        let piece = Piece::Region(Region {
            id: id.to_string(),
            tip: title.to_string(),
            title: title.to_string(),
            text,
            min_lines: MIN_LINES,
            fill: false,
        });
        if matches!(id, "companion.stats" | "companion.skills") {
            left.push_back(piece);
        } else {
            right.push_back(piece);
        }
    }
    (left, right)
}

/// Build the Companion page (and its continuation pages). Empty without a stat block.
pub(crate) fn build(data: &SheetData, opts: &LayoutOptions, grid: &Grid) -> Vec<PdfPage> {
    let Some(companion) = &data.extras.companion else {
        return Vec::new();
    };
    let env = Env {
        d: data,
        o: opts,
        g: grid,
    };
    let title = companion.title();
    let continued = format!("{title} (continued)");
    let mut seq = Seq::default();
    let (mut left, mut right) = queues(data);
    let (mut c, top) = new_canvas(&env, Some(&title));
    let [a, b] = columns(grid);
    {
        let mut cx = Cx {
            c: &mut c,
            ids: &mut seq.ids,
            d: data,
            ordinal: 1,
            page_idx: 0,
            refs: &mut seq.refs,
            landed: &mut seq.landed,
        };
        fill(&mut cx, &mut left, (a.0, top, a.1), grid.body_bottom);
        fill(&mut cx, &mut right, (b.0, top, b.1), grid.body_bottom);
    }
    seq.push(c);
    seq.flow_pages(&env, 1, (&continued, &continued), left, right);
    seq.pages
}
