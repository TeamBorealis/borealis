//! Story page (2024 page 4): portrait, appearance, alignment, personality, backstory, allies and
//! organisation, notes. Printed only when the story has content. On 2014 sheets the story
//! fields live on the Details page ([`super::page_features`]), so this builds nothing.

#[path = "story_regions.rs"]
pub(super) mod story_regions;

use crate::layout::grid::{Grid, Rect};
use crate::layout::page_features::features_flow::{fill, Cx, HEAD_H, PORTRAIT_H, PORTRAIT_W};
use crate::layout::page_features::features_pages::{columns, new_canvas, Env, Seq};
use crate::layout::LayoutOptions;
use crate::model_types::{Edition, SheetData};
use crate::pdf::PdfPage;

/// Where a portrait image goes on the Story page (page 1 of the returned pages).
pub(crate) fn portrait_rect(g: &Grid, compact: bool) -> Rect {
    let k = if compact {
        crate::layout::grid::COMPACT_K
    } else {
        1.0
    };
    let (x, _) = columns(g)[0];
    (x, g.body_top + HEAD_H * k, PORTRAIT_W, PORTRAIT_H)
}

/// Build the Story page (and its continuation pages). Empty when there is nothing to print or
/// the edition is 2014.
pub(crate) fn build(
    data: &SheetData,
    opts: &LayoutOptions,
    grid: &Grid,
    first_page_no: u32,
) -> Vec<PdfPage> {
    build_with_limits(data, opts, grid, first_page_no).0
}

/// [`build`] plus the width limit of every text op, per page.
pub(crate) fn build_with_limits(
    data: &SheetData,
    opts: &LayoutOptions,
    grid: &Grid,
    first_page_no: u32,
) -> (Vec<PdfPage>, Vec<Vec<(usize, f32)>>) {
    let _ = first_page_no;
    if data.edition != Edition::Dnd2024 || !data.story.has_content() {
        return (Vec::new(), Vec::new());
    }
    let env = Env {
        d: data,
        o: opts,
        g: grid,
    };
    let mut seq = Seq::default();
    let (mut left, mut right) = story_regions::story_2024(data);
    let (mut c, top) = new_canvas(&env, None);
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
    seq.flow_pages(
        &env,
        1,
        ("Story (continued)", "Story (continued)"),
        left,
        right,
    );
    (seq.pages, seq.limits)
}
