//! Placing generated art pieces (`art::piece`) as vector paths on a canvas.

use super::grid::Canvas;
use crate::art::{self, ArtId, ArtPiece, ArtSet};
use crate::pdf::{art_form_ok, art_path_paint, PageOp};
use crate::pdf_draw::{Path, Transform};

/// Draw `piece` with its origin at `(x, y)` and `(sx, sy)` points per unit (negative mirrors).
/// Placements that keep strokes undistorted go through one shared Form XObject per piece;
/// others (a stretched ribbon with strokes) emit the paths directly.
pub(crate) fn draw_piece(
    c: &mut Canvas,
    piece: &'static ArtPiece,
    (x, y): (f32, f32),
    (sx, sy): (f32, f32),
) {
    if art_form_ok(piece, sx, sy) {
        c.push(PageOp::Art {
            piece,
            x,
            y,
            sx,
            sy,
        });
        return;
    }
    let t = Transform::place(x, y, sx, sy);
    let line_scale = (sx.abs() + sy.abs()) / 2.0;
    for p in piece.paths {
        let (path, paint) = art_path_paint(p, line_scale);
        c.path(path.transformed(t), paint);
    }
}

/// Art set for the mode.
pub(crate) fn set(ink: bool) -> ArtSet {
    if ink {
        ArtSet::Ink
    } else {
        ArtSet::Colour
    }
}

/// Draw a piece by id with uniform scale.
pub(crate) fn place(c: &mut Canvas, ink: bool, id: ArtId, at: (f32, f32), s: f32) {
    draw_piece(c, art::piece(set(ink), id), at, (s, s));
}

/// Draw a piece clipped to a rectangle and stretched to fill it.
pub(crate) fn place_clipped(
    c: &mut Canvas,
    ink: bool,
    id: ArtId,
    (x, y, w, h): (f32, f32, f32, f32),
) {
    let piece = art::piece(set(ink), id);
    c.push(PageOp::Save);
    c.push(PageOp::Clip {
        path: Path::rect(x, y, w, h),
        even_odd: false,
    });
    draw_piece(c, piece, (x, y), (w / piece.width, h / piece.height));
    c.push(PageOp::Restore);
}
