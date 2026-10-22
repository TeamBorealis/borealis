//! Page sequence plumbing shared by the features, details, inventory and story pages: new pages
//! with chrome, two-column continuation flow, and the "full text on page N" notes.

use super::features_flow::{continued, fill, fill_last, ContMark, Cx, DetailRef, Piece, MARK_SIZE};
use super::features_text::Ids;
use crate::fonts::Style;
use crate::layout::grid::{Align, Canvas, Grid, Palette, Txt, MARGIN_X};
use crate::layout::{chrome, LayoutOptions};
use crate::model_types::SheetData;
use crate::pdf::PdfPage;
use std::collections::VecDeque;

/// Gap between the two body columns of these pages.
pub(crate) const COL_GAP: f32 = 14.0;

/// Everything a page sequence needs.
pub(crate) struct Env<'a> {
    pub d: &'a SheetData,
    pub o: &'a LayoutOptions,
    pub g: &'a Grid,
}

/// Mutable state of one page sequence.
#[derive(Default)]
pub(crate) struct Seq {
    pub pages: Vec<PdfPage>,
    pub ids: Ids,
    pub refs: Vec<DetailRef>,
    pub landed: Vec<(usize, usize)>,
    /// "continued on page N" notes, resolved with the page numbers.
    pub marks: Vec<ContMark>,
    /// Width limit of every text op, per page (for tests).
    pub limits: Vec<Vec<(usize, f32)>>,
}

/// A queue that goes on on a new page in the middle of a group ("Feats") starts with a
/// "Feats (continued)" heading.
fn regroup(q: &mut VecDeque<Piece>) {
    if let Some(Piece::Feature(f)) = q.front() {
        if !f.group.is_empty() {
            let title = continued(&f.group);
            q.push_front(Piece::Heading { title, hint: None });
        }
    }
}

/// The two body columns `[(x, w); 2]`.
pub(crate) fn columns(g: &Grid) -> [(f32, f32); 2] {
    let w = (g.content_w() - COL_GAP) / 2.0;
    [(MARGIN_X, w), (MARGIN_X + w + COL_GAP, w)]
}

/// A new page canvas with the chrome and an optional banner. Returns it with the body top.
pub(crate) fn new_canvas(env: &Env, banner: Option<&str>) -> (Canvas, f32) {
    let mut c = Canvas::new(Palette::new(env.o.ink), env.o.compact, env.g.w - MARGIN_X);
    chrome::furniture(&mut c, env.g, env.o);
    chrome::header(&mut c, env.g, env.d);
    let mut top = env.g.body_top;
    if let Some(title) = banner {
        top = c.heading(MARGIN_X, top, env.g.content_w(), title, None) + 2.0;
    }
    (c, top)
}

impl Seq {
    pub fn push(&mut self, c: Canvas) {
        self.limits.push(c.limits);
        self.pages.push(PdfPage {
            ops: c.ops,
            struct_parents: None,
        });
    }

    /// Append pages until both queues are empty. A queue that outlives the other flows through
    /// both columns. `ordinal` is the ordinal of the last page already built; `first`/`rest`
    /// are the banner titles of the first and of every further page.
    pub fn flow_pages(
        &mut self,
        env: &Env,
        mut ordinal: u32,
        (first, rest): (&str, &str),
        mut left: VecDeque<Piece>,
        mut right: VecDeque<Piece>,
    ) {
        let mut n = 0;
        while !left.is_empty() || !right.is_empty() {
            ordinal += 1;
            n += 1;
            regroup(&mut left);
            regroup(&mut right);
            let (mut c, top) = new_canvas(env, Some(if n == 1 { first } else { rest }));
            let [a, b] = columns(env.g);
            let bottom = env.g.body_bottom;
            let mut cx = Cx {
                c: &mut c,
                ids: &mut self.ids,
                d: env.d,
                ordinal,
                page_idx: self.pages.len(),
                refs: &mut self.refs,
                landed: &mut self.landed,
            };
            let marks = &mut self.marks;
            if left.is_empty() {
                fill(&mut cx, &mut right, (a.0, top, a.1), bottom);
                fill_last(&mut cx, &mut right, (b.0, top, b.1), bottom, marks);
            } else if right.is_empty() {
                fill(&mut cx, &mut left, (a.0, top, a.1), bottom);
                fill_last(&mut cx, &mut left, (b.0, top, b.1), bottom, marks);
            } else {
                fill_last(&mut cx, &mut left, (a.0, top, a.1), bottom, marks);
                fill_last(&mut cx, &mut right, (b.0, top, b.1), bottom, marks);
            }
            self.push(c);
        }
    }

    /// Add the "continued on page N" notes; `page_no(i)` maps a page index of this sequence to
    /// its printed page number.
    pub fn resolve_marks(&mut self, o: &LayoutOptions, page_no: impl Fn(usize) -> u32) {
        draw_marks(
            &mut self.pages,
            &std::mem::take(&mut self.marks),
            o,
            page_no,
        );
    }

    /// Add "full text on page N" notes now that the page of each detail block is known.
    /// `page_no(i)` maps a page index of this sequence to its printed page number.
    pub fn resolve_refs(&mut self, env: &Env, page_no: impl Fn(usize) -> u32) {
        for r in std::mem::take(&mut self.refs) {
            let Some(&(_, at)) = self.landed.iter().find(|(n, _)| *n == r.n) else {
                continue;
            };
            let mut c = Canvas::new(Palette::new(env.o.ink), env.o.compact, env.g.w);
            let t = Txt::new(c.s(5.8), Style::SansItalic, c.pal.ink2).align(Align::End);
            c.put(
                r.x_right,
                r.base,
                &t,
                &format!("full text on page {}", page_no(at)),
            );
            self.pages[r.page].ops.extend(c.ops);
        }
    }
}

/// How far a note in the last column moves left to clear the corner flourish, in points.
const CORNER_CLEAR: f32 = 26.0;

/// Draw "continued on page N" notes into `pages` (N is the page after the mark's own).
pub(crate) fn draw_marks(
    pages: &mut [PdfPage],
    marks: &[ContMark],
    o: &LayoutOptions,
    page_no: impl Fn(usize) -> u32,
) {
    for m in marks {
        let Some(page) = pages.get_mut(m.page) else {
            continue;
        };
        let mut c = Canvas::new(Palette::new(o.ink), o.compact, f32::MAX);
        let t = Txt::new(c.s(MARK_SIZE), Style::SansItalic, c.pal.ink2).align(Align::End);
        let note = format!("continued on page {}", page_no(m.page + 1));
        // The note of the last column would run into the bottom-right corner flourish.
        let right_edge = Grid::new(o.paper).w - MARGIN_X;
        let x_right = if m.x_right >= right_edge - 1.0 {
            m.x_right - CORNER_CLEAR
        } else {
            m.x_right
        };
        c.put(x_right, m.base, &t, &note);
        page.ops.extend(c.ops);
    }
}
