//! Vector drawing primitives shared by the PDF writer, form appearances and layout: colours,
//! paths (with an affine placement), paint (fill, stroke, dash, opacity) and number formatting
//! with the typographic minus.

use std::borrow::Cow;
use std::fmt::Write as _;

/// Default stroke width of rectangles, in points.
pub(crate) const HAIRLINE: f32 = 0.5;
/// Alpha is quantised to 1/1000 so equal opacities share one ExtGState.
pub(crate) const ALPHA_STEPS: f32 = 1000.0;

/// An RGB colour, each channel 0..=1.
pub(crate) type Rgb = (f32, f32, f32);

/// The typographic minus sign (U+2212) used for negative numbers.
pub(crate) const MINUS: char = '\u{2212}';

/// A signed number with an explicit sign: `+3`, `+0`, `−1` (U+2212).
pub(crate) fn signed(n: i64) -> String {
    if n < 0 {
        format!("{MINUS}{}", n.unsigned_abs())
    } else {
        format!("+{n}")
    }
}

/// Replace a hyphen-minus that starts a number token (`-1`, `(-2)`, `1d6 -1`) with U+2212.
/// Hyphens inside words or ranges (`20-60`, `x-ray`, a lone `-`) are kept.
pub(crate) fn typographic_minus(text: &str) -> Cow<'_, str> {
    if !text.contains('-') {
        return Cow::Borrowed(text);
    }
    let chars: Vec<char> = text.chars().collect();
    let out: String = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let starts_token = i == 0 || matches!(chars[i - 1], ' ' | '(' | '[' | '/' | '\u{a0}');
            let before_digit = chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if c == '-' && starts_token && before_digit {
                MINUS
            } else {
                c
            }
        })
        .collect();
    Cow::Owned(out)
}

// ---------------------------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------------------------

/// One path segment in path space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    /// Cubic Bézier: control 1, control 2, end point.
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Affine map from path space to page space: `(a x + c y + e, b x + d y + f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Scale by `(sx, sy)` then move the origin to `(x, y)`. A negative scale mirrors.
    pub fn place(x: f32, y: f32, sx: f32, sy: f32) -> Transform {
        Transform {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: x,
            f: y,
        }
    }

    /// `self` applied after `inner`.
    pub fn then_after(self, inner: Transform) -> Transform {
        Transform {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }

    pub fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

/// A vector path. Static tables (the generated art) borrow their segments without copying.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Path {
    pub segs: Cow<'static, [Seg]>,
    pub transform: Transform,
}

/// Bézier handle length for a quarter circle of radius 1.
const KAPPA: f32 = 0.552_284_8;

impl Path {
    pub fn new(segs: Vec<Seg>) -> Path {
        Path {
            segs: Cow::Owned(segs),
            transform: Transform::IDENTITY,
        }
    }

    /// The same path with `t` applied after its current transform.
    #[must_use]
    pub fn transformed(self, t: Transform) -> Path {
        Path {
            transform: t.then_after(self.transform),
            ..self
        }
    }

    pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Path {
        Path::polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
    }

    /// A closed polygon through `points`.
    pub fn polygon(points: &[(f32, f32)]) -> Path {
        let mut p = Path::polyline(points);
        if let Cow::Owned(segs) = &mut p.segs {
            segs.push(Seg::Close);
        }
        p
    }

    /// An open polyline through `points`.
    pub fn polyline(points: &[(f32, f32)]) -> Path {
        let segs = points
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                if i == 0 {
                    Seg::Move(x, y)
                } else {
                    Seg::Line(x, y)
                }
            })
            .collect();
        Path::new(segs)
    }

    pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
        let r = r.clamp(0.0, w.min(h) / 2.0);
        if r <= 0.0 {
            return Path::rect(x, y, w, h);
        }
        let k = r * (1.0 - KAPPA);
        let (x2, y2) = (x + w, y + h);
        Path::new(vec![
            Seg::Move(x + r, y),
            Seg::Line(x2 - r, y),
            Seg::Cubic(x2 - k, y, x2, y + k, x2, y + r),
            Seg::Line(x2, y2 - r),
            Seg::Cubic(x2, y2 - k, x2 - k, y2, x2 - r, y2),
            Seg::Line(x + r, y2),
            Seg::Cubic(x + k, y2, x, y2 - k, x, y2 - r),
            Seg::Line(x, y + r),
            Seg::Cubic(x, y + k, x + k, y, x + r, y),
            Seg::Close,
        ])
    }

    pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        Path::new(vec![
            Seg::Move(cx + rx, cy),
            Seg::Cubic(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry),
            Seg::Cubic(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy),
            Seg::Cubic(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry),
            Seg::Cubic(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy),
            Seg::Close,
        ])
    }

    pub fn circle(cx: f32, cy: f32, r: f32) -> Path {
        Path::ellipse(cx, cy, r, r)
    }

    /// A diamond (rhombus) centred on `(cx, cy)` with half-diagonals `rx`, `ry`.
    pub fn diamond(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
        Path::polygon(&[(cx, cy - ry), (cx + rx, cy), (cx, cy + ry), (cx - rx, cy)])
    }
}

/// How a path is painted. The default paints nothing; combine the builders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Paint {
    pub fill: Option<Rgb>,
    pub stroke: Option<Rgb>,
    /// Stroke width in points (scaled by nothing: the transform applies to geometry only).
    pub width: f32,
    /// Fill and stroke opacity, 0..=1 (an ExtGState when below 1).
    pub opacity: f32,
    /// Even-odd fill rule instead of non-zero winding.
    pub even_odd: bool,
    /// Round caps and joins.
    pub round: bool,
    /// Dash pattern `[on, off]` in points.
    pub dash: Option<[f32; 2]>,
}

impl Default for Paint {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            width: HAIRLINE,
            opacity: 1.0,
            even_odd: false,
            round: false,
            dash: None,
        }
    }
}

impl Paint {
    pub fn fill(color: Rgb) -> Paint {
        Paint {
            fill: Some(color),
            ..Paint::default()
        }
    }

    pub fn stroke(color: Rgb, width: f32) -> Paint {
        Paint {
            stroke: Some(color),
            width,
            ..Paint::default()
        }
    }

    #[must_use]
    pub fn with_stroke(self, color: Rgb, width: f32) -> Paint {
        Paint {
            stroke: Some(color),
            width,
            ..self
        }
    }

    #[must_use]
    pub fn with_opacity(self, opacity: f32) -> Paint {
        Paint { opacity, ..self }
    }

    #[must_use]
    #[cfg_attr(not(test), allow(dead_code))] // exercised by tests; kept as API
    pub fn even_odd(self) -> Paint {
        Paint {
            even_odd: true,
            ..self
        }
    }

    #[must_use]
    pub fn rounded(self) -> Paint {
        Paint {
            round: true,
            ..self
        }
    }

    #[must_use]
    pub fn dashed(self, on: f32, off: f32) -> Paint {
        Paint {
            dash: Some([on, off]),
            ..self
        }
    }
}

pub(crate) fn color(c: Rgb) -> String {
    format!("{:.3} {:.3} {:.3}", c.0, c.1, c.2)
}

pub(crate) fn alpha_key(a: f32) -> u16 {
    (a.clamp(0.0, 1.0) * ALPHA_STEPS).round() as u16
}

/// `/GA350 gs` for opacity 0.35 (`None` when fully opaque).
pub(crate) fn gs_name(a: f32) -> Option<String> {
    let k = alpha_key(a);
    (k < 1000).then(|| format!("GA{k}"))
}

/// Append the operators of `path`, mapping y with `flip_h - y` (page or appearance height).
pub(crate) fn path_ops(out: &mut String, path: &Path, flip_h: f32) {
    let t = path.transform;
    let p = |x: f32, y: f32| {
        let (x, y) = t.apply(x, y);
        format!("{x:.2} {:.2}", flip_h - y)
    };
    for seg in path.segs.iter() {
        match *seg {
            Seg::Move(x, y) => {
                let _ = write!(out, "{} m ", p(x, y));
            }
            Seg::Line(x, y) => {
                let _ = write!(out, "{} l ", p(x, y));
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => {
                let _ = write!(out, "{} {} {} c ", p(x1, y1), p(x2, y2), p(x, y));
            }
            Seg::Close => out.push_str("h "),
        }
    }
}

/// Paint `path` in its own `q ... Q` so no state leaks.
pub(crate) fn paint_path(out: &mut String, path: &Path, paint: &Paint, flip_h: f32) {
    let op = match (paint.fill.is_some(), paint.stroke.is_some(), paint.even_odd) {
        (true, true, false) => "B",
        (true, true, true) => "B*",
        (true, false, false) => "f",
        (true, false, true) => "f*",
        (false, true, _) => "S",
        (false, false, _) => return,
    };
    out.push_str("q ");
    if let Some(gs) = gs_name(paint.opacity) {
        let _ = write!(out, "/{gs} gs ");
    }
    if let Some(f) = paint.fill {
        let _ = write!(out, "{} rg ", color(f));
    }
    if let Some(s) = paint.stroke {
        let _ = write!(out, "{} RG {:.2} w ", color(s), paint.width);
        if paint.round {
            out.push_str("1 J 1 j ");
        }
        if let Some([on, off]) = paint.dash {
            let _ = write!(out, "[{on:.2} {off:.2}] 0 d ");
        }
    }
    path_ops(out, path, flip_h);
    out.push_str(op);
    out.push_str(" Q\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_numbers_use_the_minus_sign() {
        assert_eq!(signed(3), "+3");
        assert_eq!(signed(0), "+0");
        assert_eq!(signed(-12), "\u{2212}12");
    }

    #[test]
    fn typographic_minus_only_touches_number_tokens() {
        assert_eq!(typographic_minus("-1"), "\u{2212}1");
        assert_eq!(
            typographic_minus("1d6 -1 (-2)"),
            "1d6 \u{2212}1 (\u{2212}2)"
        );
        assert_eq!(
            typographic_minus("20-60 ft, x-ray, -"),
            "20-60 ft, x-ray, -"
        );
        assert!(matches!(typographic_minus("no hyphen"), Cow::Borrowed(_)));
    }

    #[test]
    fn transforms_compose_and_mirror() {
        let t = Transform::place(10.0, 20.0, 2.0, 2.0);
        assert_eq!(t.apply(1.0, 1.0), (12.0, 22.0));
        let mirrored = Transform::place(100.0, 0.0, -1.0, 1.0).then_after(t);
        assert_eq!(mirrored.apply(1.0, 1.0), (88.0, 22.0));
        let mut out = String::new();
        path_ops(&mut out, &Path::rect(0.0, 0.0, 10.0, 5.0), 100.0);
        assert_eq!(
            out,
            "0.00 100.00 m 10.00 100.00 l 10.00 95.00 l 0.00 95.00 l h "
        );
    }

    #[test]
    fn paint_wraps_state_and_uses_named_alpha() {
        let mut out = String::new();
        let paint = Paint::fill((1.0, 0.0, 0.0))
            .with_stroke((0.0, 0.0, 0.0), 1.0)
            .with_opacity(0.35)
            .even_odd()
            .dashed(2.0, 1.0)
            .rounded();
        paint_path(&mut out, &Path::circle(5.0, 5.0, 5.0), &paint, 10.0);
        assert!(out.starts_with("q /GA350 gs 1.000 0.000 0.000 rg"), "{out}");
        assert!(out.contains("1 J 1 j [2.00 1.00] 0 d"));
        assert!(out.trim_end().ends_with("B* Q"));
        let mut none = String::new();
        paint_path(
            &mut none,
            &Path::circle(5.0, 5.0, 5.0),
            &Paint::default(),
            10.0,
        );
        assert!(none.is_empty());
        assert_eq!(gs_name(1.0), None);
    }
}
