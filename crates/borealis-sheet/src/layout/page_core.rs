//! Page 1 (Character) for both editions.

use super::grid::{Canvas, Col, Grid, Palette, MARGIN_X};
use super::lists;
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

/// Build page 1. `continuation` is the page number printed in "+N more on page X".
pub(crate) fn build(d: &SheetData, o: &LayoutOptions, g: &Grid, continuation: u32) -> Page1 {
    let mut c = Canvas::new(Palette::new(o.ink), o.compact, g.w - MARGIN_X);
    chrome::furniture(&mut c, g, o);
    chrome::header(&mut c, g, d);
    abilities::column(&mut c, g, d, o);
    let (x, w) = g.column(Col::End);
    let mut y = g.body_top;
    y += vitals::vitals(&mut c, (x, y, w), d) + ROW_GAP;
    y += vitals::strip(&mut c, (x, y, w), d) + 10.0;
    let (ny, _) = lists::weapons(&mut c, (x, y, w), d, continuation);
    y = lists::senses(&mut c, (x, ny, w), d);
    y = lists::training(&mut c, (x, y, w), d);
    y = lists::conditions(&mut c, (x, y, w), d);
    let (ny, _) = lists::limited_uses(&mut c, (x, y, w), d, continuation);
    y = ny;
    if d.edition == Edition::Dnd2014 {
        y = lists::features_summary(&mut c, (x, y, w), d, continuation);
    }
    lists::notes(&mut c, (x, y, w), g);
    Page1 {
        page: PdfPage {
            ops: c.ops,
            struct_parents: None,
        },
        limits: c.limits,
    }
}
