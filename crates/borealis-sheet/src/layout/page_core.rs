//! Page 1 (Character) for both editions.

use super::grid::{Canvas, Col, Grid, Palette, MARGIN_X};
use super::lists;
use super::p1_band::{self, Band, BandRefs};
use super::LayoutOptions;
use super::{abilities, chrome, vitals};
use crate::model_types::{Edition, SheetData};
use crate::pdf::PdfPage;

/// Gap between the vitals row, the strip and the first list.
const ROW_GAP: f32 = 6.0;

/// A built page 1.
pub(crate) struct Page1 {
    pub page: PdfPage,
    /// `(op index, max width)` of every text op.
    #[cfg_attr(not(test), allow(dead_code))] // read by the layout tests
    pub limits: Vec<(usize, f32)>,
}

/// Page numbers page 1 points at.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageRefs {
    /// Printed in "+N more on page X" for attacks, limited uses and features.
    pub continuation: u32,
    /// The 2014 band's Details and Inventory pages.
    pub band: BandRefs,
}

/// Build page 1. `band` is the 2014 personality and equipment band ([`band`]); without it the
/// end column closes with Notes.
pub(crate) fn build(
    d: &SheetData,
    o: &LayoutOptions,
    g: &Grid,
    refs: PageRefs,
    band: Option<&Band>,
) -> Page1 {
    let mut c = canvas(o, g);
    chrome::furniture(&mut c, g, o);
    chrome::header(&mut c, g, d);
    abilities::column(&mut c, g, d, o);
    let (x, w) = g.column(Col::End);
    let y = end_column(&mut c, d, g, refs.continuation);
    match band {
        Some(b) => p1_band::draw(&mut c, d, b, (x, w), refs.band),
        None => lists::notes(&mut c, (x, y, w), g),
    }
    Page1 {
        page: PdfPage {
            ops: c.ops,
            struct_parents: None,
        },
        limits: c.limits,
    }
}

/// The 2014 band that closes the end column of page 1, measured below the sections above it;
/// `None` for 2024 or when there is no room (see [`p1_band::plan`]).
pub(crate) fn band(d: &SheetData, o: &LayoutOptions, g: &Grid) -> Option<Band> {
    if d.edition != Edition::Dnd2014 {
        return None;
    }
    let mut c = canvas(o, g);
    let top = end_column(&mut c, d, g, 1);
    p1_band::plan(d, c.k, g.column(Col::End), top, g.body_bottom)
}

fn canvas(o: &LayoutOptions, g: &Grid) -> Canvas {
    Canvas::new(Palette::new(o.ink), o.compact, g.w - MARGIN_X)
}

/// The end column down to the Notes or the band; returns the y below.
fn end_column(c: &mut Canvas, d: &SheetData, g: &Grid, continuation: u32) -> f32 {
    let (x, w) = g.column(Col::End);
    let mut y = g.body_top;
    y += vitals::vitals(c, (x, y, w), d) + ROW_GAP;
    y += vitals::strip(c, (x, y, w), d) + 10.0;
    let (ny, _) = lists::weapons(c, (x, y, w), d, continuation);
    y = lists::senses(c, (x, ny, w), d);
    y = lists::training(c, (x, y, w), d);
    y = lists::conditions(c, (x, y, w), d);
    let (ny, _) = lists::limited_uses(c, (x, y, w), d, continuation);
    y = ny;
    if d.edition == Edition::Dnd2014 {
        y = lists::features_summary(c, (x, y, w), d, continuation);
    }
    y
}
