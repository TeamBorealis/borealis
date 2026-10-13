//! Page grid, palette and the [`Canvas`] every page builder draws on.
//!
//! Coordinates are page space (top-left origin, y down, points). Columns are addressed by
//! logical start/end (`Col`), so mirroring a sheet for a right-to-left locale is a flag.

use crate::fonts::{text_width, Style};
use crate::pdf::{PageOp, BASELINE};
use crate::pdf_draw::{typographic_minus, Paint, Path, Rgb};
use crate::pdf_form::{
    CheckField, FieldFrame, FieldKind, FormField, Mark, Shape, TextAlign, TextField,
};
use crate::Paper;

/// Left and right page margin of the content area.
pub(crate) const MARGIN_X: f32 = 42.0;
/// Top of the body, below the header divider.
pub(crate) const BODY_TOP: f32 = 101.0;
/// Distance of the body bottom from the page bottom.
pub(crate) const BODY_BOTTOM_GAP: f32 = 50.0;
/// Width of the start column (abilities).
pub(crate) const START_COL_W: f32 = 178.0;
/// Gap between the two columns.
pub(crate) const COL_GAP: f32 = 10.0;
/// Label size (5.4 pt caps, tracked).
pub(crate) const LABEL_SIZE: f32 = 5.4;
/// Label tracking, in em.
pub(crate) const LABEL_TRACK: f32 = 0.09;
/// Size scale of compact mode.
pub(crate) const COMPACT_K: f32 = 0.93;
/// Pad inside single-line fields.
pub(crate) const FIELD_PAD: f32 = 1.0;

/// Descender depth of the body face, as a fraction of the font size.
const DESCENDER: f32 = 0.27;
/// Air kept between descenders and the rule below, in points.
const RULE_CLEARANCE: f32 = 2.2;

/// First baseline of a ruled field, from its top: one pitch down minus descender room.
#[must_use]
pub(crate) fn ruled_baseline(pitch: f32, size: f32) -> f32 {
    pitch - size * DESCENDER - RULE_CLEARANCE
}
/// Smallest type a fitted text may shrink to, as a fraction of its size.
const MIN_FIT: f32 = 0.78;

/// A rectangle `(x, y, w, h)`.
pub(crate) type Rect = (f32, f32, f32, f32);

/// Logical column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Col {
    /// The column at the reading start (left in LTR): abilities.
    Start,
    /// The column at the reading end: the main flow.
    End,
}

/// Page geometry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Grid {
    pub w: f32,
    pub h: f32,
    /// Mirror the columns (right-to-left locales; not yet used by any page).
    pub rtl: bool,
    pub body_top: f32,
    pub body_bottom: f32,
}

impl Grid {
    pub fn new(paper: Paper) -> Grid {
        let (w, h) = paper.points();
        Grid {
            w,
            h,
            rtl: false,
            body_top: BODY_TOP,
            body_bottom: h - BODY_BOTTOM_GAP,
        }
    }

    /// Content width.
    pub fn content_w(&self) -> f32 {
        self.w - 2.0 * MARGIN_X
    }

    /// Physical `(x, width)` of a logical column.
    pub fn column(&self, col: Col) -> (f32, f32) {
        let end_w = self.content_w() - START_COL_W - COL_GAP;
        let (start_x, end_x) = if self.rtl {
            (self.w - MARGIN_X - START_COL_W, MARGIN_X)
        } else {
            (MARGIN_X, MARGIN_X + START_COL_W + COL_GAP)
        };
        match col {
            Col::Start => (start_x, START_COL_W),
            Col::End => (end_x, end_w),
        }
    }
}

/// The sheet palette (§5). Ink mode keeps greys only.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Palette {
    /// Ink (monochrome) mode: the art uses its ink set.
    pub ink_mode: bool,
    pub paper: Rgb,
    pub ink: Rgb,
    pub ink2: Rgb,
    pub rule: Rgb,
    pub tint: Rgb,
    pub gold: Rgb,
    pub teal: Rgb,
}

fn hex(v: u32) -> Rgb {
    let c = |shift: u32| ((v >> shift) & 0xff) as f32 / 255.0;
    (c(16), c(8), c(0))
}

fn grey(v: f32) -> Rgb {
    (v, v, v)
}

impl Palette {
    pub fn new(ink_mode: bool) -> Palette {
        if ink_mode {
            Palette {
                ink_mode: true,
                paper: grey(1.0),
                ink: grey(0.1),
                ink2: grey(0.33),
                rule: grey(0.72),
                tint: grey(0.95),
                gold: grey(0.3),
                teal: grey(0.55),
            }
        } else {
            Palette {
                ink_mode: false,
                paper: hex(0xFBF9F4),
                ink: hex(0x1F272B),
                ink2: hex(0x4E585D),
                rule: hex(0xCFC9BC),
                tint: hex(0xF3EFE6),
                gold: hex(0xB38000),
                teal: hex(0x305754),
            }
        }
    }
}

/// Horizontal alignment of drawn text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Start,
    Center,
    End,
}

impl From<Align> for TextAlign {
    fn from(a: Align) -> Self {
        match a {
            Align::Start => TextAlign::Start,
            Align::Center => TextAlign::Center,
            Align::End => TextAlign::End,
        }
    }
}

/// How a piece of text is drawn.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Txt {
    pub size: f32,
    pub style: Style,
    pub color: Rgb,
    pub align: Align,
    /// Maximum width; text shrinks (down to `MIN_FIT`) and then truncates with an ellipsis.
    pub within: Option<f32>,
}

impl Txt {
    pub fn new(size: f32, style: Style, color: Rgb) -> Txt {
        Txt {
            size,
            style,
            color,
            align: Align::Start,
            within: None,
        }
    }

    #[must_use]
    pub fn align(self, align: Align) -> Txt {
        Txt { align, ..self }
    }

    #[must_use]
    pub fn within(self, w: f32) -> Txt {
        Txt {
            within: Some(w),
            ..self
        }
    }

    #[must_use]
    pub fn sized(self, size: f32) -> Txt {
        Txt { size, ..self }
    }

    #[must_use]
    pub fn colored(self, color: Rgb) -> Txt {
        Txt { color, ..self }
    }
}

/// Greedy word wrap to `max` points.
pub(crate) fn wrap_lines(style: Style, size: f32, max: f32, text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let trial = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || text_width(style, &trial, size) <= max {
            cur = trial;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Width of caps text with tracking.
pub(crate) fn tracked_width(style: Style, text: &str, size: f32) -> f32 {
    let n = text.chars().count() as f32;
    text_width(style, text, size) + (n - 1.0).max(0.0) * LABEL_TRACK * size
}

/// Largest size `<= size` (not below `min`) at which `text` fits `max`.
pub(crate) fn fit_size(style: Style, text: &str, size: f32, min: f32, max: f32) -> f32 {
    let w = text_width(style, text, size);
    if w <= max || w <= 0.0 {
        size
    } else {
        (size * max / w).max(min)
    }
}

/// Smallest size a field value shrinks to before it is cut with an ellipsis (design §5: body
/// 7.4 pt, labels 5.4 pt; a value set smaller than its design size never goes below 6 pt).
pub(crate) const LEGIBLE_MIN: f32 = 6.0;

/// `text` wrapped to `width` and cut to `max_lines`, the last kept line ending in an ellipsis
/// when lines were cut. Lines are joined with `\n` (each already fits, so it re-wraps the same).
pub(crate) fn cut_lines(
    text: &str,
    style: Style,
    size: f32,
    width: f32,
    max_lines: usize,
) -> String {
    let mut lines = crate::pdf_form::wrap(text, style, size, width);
    if lines.len() > max_lines.max(1) {
        lines.truncate(max_lines.max(1));
        if let Some(last) = lines.last_mut() {
            let mut s: String = last.trim_end().to_string();
            s.push('\u{2026}');
            if text_width(style, &s, size) > width {
                s = truncate(style, last.trim_end(), size, width);
            }
            *last = s;
        }
    }
    lines.join("\n")
}

/// Truncate with an ellipsis so the width is at most `max`.
pub(crate) fn truncate(style: Style, text: &str, size: f32, max: f32) -> String {
    let mut out: Vec<char> = text.chars().collect();
    while !out.is_empty() {
        let s: String = out.iter().collect::<String>() + "\u{2026}";
        if text_width(style, &s, size) <= max {
            return s;
        }
        out.pop();
    }
    String::new()
}

/// Operations of one page plus the width limit of every text op (for tests).
pub(crate) struct Canvas {
    pub pal: Palette,
    /// Size scale (compact mode).
    pub k: f32,
    /// Page right edge of the content, the default width limit.
    pub right_edge: f32,
    pub ops: Vec<PageOp>,
    /// `(op index, max width)` for every drawn text op.
    pub limits: Vec<(usize, f32)>,
    /// Inside [`Canvas::artifact`]: structure markers are suppressed.
    in_artifact: bool,
    /// A `Table` group is open (see [`Canvas::open_table`]).
    table_open: bool,
    /// Structure role of [`Canvas::label`] (`TH` inside a table header row).
    label_role: &'static str,
}

impl Canvas {
    pub fn new(pal: Palette, compact: bool, right_edge: f32) -> Canvas {
        Canvas {
            pal,
            k: if compact { COMPACT_K } else { 1.0 },
            right_edge,
            ops: Vec::new(),
            limits: Vec::new(),
            in_artifact: false,
            table_open: false,
            label_role: "Lbl",
        }
    }

    /// `v` scaled for compact mode.
    pub fn s(&self, v: f32) -> f32 {
        v * self.k
    }

    pub fn push(&mut self, op: PageOp) {
        self.ops.push(op);
    }

    pub fn path(&mut self, path: Path, paint: Paint) {
        self.ops.push(PageOp::Path { path, paint });
    }

    pub fn artifact(&mut self, f: impl FnOnce(&mut Canvas)) {
        self.ops.push(PageOp::BeginArtifact);
        let outer = std::mem::replace(&mut self.in_artifact, true);
        f(self);
        self.in_artifact = outer;
        self.ops.push(PageOp::EndMarked);
    }

    /// Content item of structure role `tag` (heading, label...); `actual` is the text screen
    /// readers get when the glyphs are spaced or split.
    pub fn leaf<R>(
        &mut self,
        tag: &'static str,
        actual: Option<String>,
        f: impl FnOnce(&mut Canvas) -> R,
    ) -> R {
        if self.in_artifact {
            return f(self);
        }
        self.ops.push(PageOp::Leaf { tag, actual });
        let r = f(self);
        self.ops.push(PageOp::EndMarked);
        r
    }

    /// Structure group (`Table`, `TR`, `L`...) around everything `f` draws.
    pub fn group<R>(&mut self, tag: &'static str, f: impl FnOnce(&mut Canvas) -> R) -> R {
        self.ops.push(PageOp::Group(tag));
        let r = f(self);
        self.ops.push(PageOp::GroupEnd);
        r
    }

    /// Open a structure group that stays open until [`Canvas::close_group`].
    pub fn open_group(&mut self, tag: &'static str) {
        self.ops.push(PageOp::Group(tag));
    }

    pub fn close_group(&mut self) {
        self.ops.push(PageOp::GroupEnd);
    }

    /// Start a `Table` (no-op while one is open); close it with [`Canvas::close_table`].
    pub fn open_table(&mut self) {
        if !self.table_open {
            self.table_open = true;
            self.open_group("Table");
        }
    }

    pub fn close_table(&mut self) {
        if std::mem::take(&mut self.table_open) {
            self.close_group();
        }
    }

    /// A table header row: a `TR` whose labels are `TH` cells.
    pub fn header_row<R>(&mut self, f: impl FnOnce(&mut Canvas) -> R) -> R {
        let outer = std::mem::replace(&mut self.label_role, "TH");
        let r = self.group("TR", f);
        self.label_role = outer;
        r
    }

    /// Draw one line of text with its baseline at `base`; returns the drawn width.
    pub fn put(&mut self, x: f32, base: f32, t: &Txt, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let text = typographic_minus(text).into_owned();
        let mut size = t.size;
        let mut s = text;
        let limit = t.within.unwrap_or_else(|| (self.right_edge - x).max(1.0));
        if let Some(max) = t.within {
            size = fit_size(t.style, &s, size, t.size * MIN_FIT, max);
            if text_width(t.style, &s, size) > max {
                s = truncate(t.style, &s, size, max);
            }
        }
        let w = text_width(t.style, &s, size);
        let left = match t.align {
            Align::Start => x,
            Align::Center => x - w / 2.0,
            Align::End => x - w,
        };
        let limit = if t.within.is_some() {
            limit
        } else {
            limit.max(w)
        };
        self.limits.push((self.ops.len(), limit));
        self.ops.push(PageOp::Text {
            x: left,
            y: base - BASELINE * size,
            size,
            style: t.style,
            color: t.color,
            text: s,
        });
        w
    }

    /// Tracked caps label (one op per letter, placed with the tracking).
    pub fn label(&mut self, x: f32, base: f32, align: Align, text: &str) -> f32 {
        self.label_in(x, base, align, text, self.pal.ink2, LABEL_SIZE * self.k)
    }

    pub fn label_in(
        &mut self,
        x: f32,
        base: f32,
        align: Align,
        text: &str,
        color: Rgb,
        size: f32,
    ) -> f32 {
        let role = self.label_role;
        self.leaf(role, Some(text.to_string()), |c| {
            c.label_letters(x, base, align, text, color, size)
        })
    }

    fn label_letters(
        &mut self,
        x: f32,
        base: f32,
        align: Align,
        text: &str,
        color: Rgb,
        size: f32,
    ) -> f32 {
        let style = Style::SansBold;
        let total = tracked_width(style, text, size);
        let mut cx = match align {
            Align::Start => x,
            Align::Center => x - total / 2.0,
            Align::End => x - total,
        };
        let step = LABEL_TRACK * size;
        for ch in text.chars() {
            let s = ch.to_string();
            let w = text_width(style, &s, size);
            if ch != ' ' {
                self.limits.push((self.ops.len(), w + 0.01));
                self.ops.push(PageOp::Text {
                    x: cx,
                    y: base - BASELINE * size,
                    size,
                    style,
                    color,
                    text: s,
                });
            }
            cx += w + step;
        }
        total
    }

    pub fn hline(&mut self, x1: f32, x2: f32, y: f32, color: Rgb, width: f32) {
        self.ops.push(PageOp::Line {
            x1,
            y1: y,
            x2,
            y2: y,
            color,
            width,
        });
    }

    pub fn four_star(&mut self, cx: f32, cy: f32, r: f32, color: Rgb) {
        let q = r * 0.28;
        let pts = [
            (cx, cy - r),
            (cx + q, cy - q),
            (cx + r, cy),
            (cx + q, cy + q),
            (cx, cy + r),
            (cx - q, cy + q),
            (cx - r, cy),
            (cx - q, cy - q),
        ];
        self.path(Path::polygon(&pts), Paint::fill(color));
    }

    /// Section heading: serif title, gold star, rule to the end of the column. Returns the y
    /// where content starts.
    pub fn heading(&mut self, x: f32, y: f32, w: f32, title: &str, hint: Option<&str>) -> f32 {
        let size = self.s(10.4);
        let base = y + self.s(9.4);
        let tw = self.leaf("H2", None, |c| {
            c.put(x, base, &Txt::new(size, Style::SerifBold, c.pal.ink), title)
        });
        let mut end = x + tw;
        if let Some(h) = hint {
            let t = Txt::new(self.s(6.4), Style::SansItalic, self.pal.ink2);
            end += 6.0 + self.put(end + 6.0, base, &t, h);
        }
        self.four_star(end + 7.0, base - 3.2, 2.4, self.pal.gold);
        self.hline(end + 13.0, x + w, base - 3.2, self.pal.rule, 0.6);
        y + self.s(17.5)
    }

    pub fn card(&mut self, r: Rect, radius: f32, fill: Option<Rgb>) {
        let (x, y, w, h) = r;
        let paint = Paint {
            fill,
            stroke: Some(self.pal.rule),
            width: 0.8,
            ..Paint::default()
        };
        self.path(Path::rounded_rect(x, y, w, h, radius), paint);
    }

    /// The tint frame of a write-in box (dashed) or of a computed value.
    pub fn frame_write(&self) -> FieldFrame {
        FieldFrame {
            shape: Shape::Square,
            paint: Paint::fill(self.pal.tint)
                .with_stroke(self.pal.ink2, 0.6)
                .dashed(2.0, 1.6),
            radius: 2.5,
        }
    }

    /// The widget of a single-line text field with its first baseline at `base`.
    fn single_line(
        &self,
        id: &str,
        tip: &str,
        r: Rect,
        base: f32,
        value: &str,
        t: &Txt,
    ) -> FormField {
        let (x, y, w, h) = r;
        let avail = w - 2.0 * FIELD_PAD;
        let mut value = typographic_minus(value).into_owned();
        let size = fit_size(t.style, &value, t.size, t.size.min(LEGIBLE_MIN), avail);
        if text_width(t.style, &value, size) > avail {
            value = truncate(t.style, &value, size, avail);
        }
        let mut tf = TextField::new(&value, size);
        tf.style = t.style;
        tf.color = t.color;
        tf.align = t.align.into();
        tf.pad = FIELD_PAD;
        tf.baseline = Some(base - y);
        FormField::new(id, x, y, w, h, FieldKind::Text(tf)).with_tooltip(tip)
    }

    /// A single-line text field with its first baseline at `base`. The value shrinks to fit.
    pub fn text_field(&mut self, id: &str, tip: &str, r: Rect, base: f32, value: &str, t: &Txt) {
        let f = self.single_line(id, tip, r, base, value, t);
        self.ops.push(PageOp::Field(f));
    }

    /// A single-line field with the tint frame: solid for a value, dashed for a write-in.
    pub fn tinted_field(&mut self, id: &str, tip: &str, r: Rect, base: f32, value: &str, t: &Txt) {
        let mut frame = self.frame_write();
        if !value.is_empty() {
            frame.paint = Paint::fill(self.pal.tint);
        }
        let f = self
            .single_line(id, tip, r, base, value, t)
            .with_frame(frame);
        self.ops.push(PageOp::Field(f));
    }

    /// A multi-line field of fixed height with its text on ruled lines every `pitch`.
    /// Baselines sit `ruled_baseline` below the line above, clear of descenders.
    pub fn ruled_field(
        &mut self,
        id: &str,
        tip: &str,
        r: Rect,
        text: &str,
        (pitch, size): (f32, f32),
    ) {
        let (x, y, w, h) = r;
        let base = ruled_baseline(pitch, size);
        let rows = crate::pdf_form::fitting_lines(base, pitch, size, h);
        let text = cut_lines(
            &typographic_minus(text),
            Style::Sans,
            size,
            (w - 2.0 * FIELD_PAD).max(1.0),
            rows,
        );
        let mut tf = TextField::new(&text, size);
        tf.style = Style::Sans;
        tf.color = self.pal.ink;
        tf.multiline = true;
        tf.leading = Some(pitch);
        tf.pad = FIELD_PAD;
        tf.baseline = Some(base);
        self.ops.push(PageOp::Field(
            FormField::new(id, x, y, w, h, FieldKind::Text(tf)).with_tooltip(tip),
        ));
        let rows = (h / pitch).floor() as usize;
        for i in 1..=rows {
            self.hline(x, x + w, y + pitch * i as f32, self.pal.rule, 0.5);
        }
    }

    /// A write-in text box (empty, tinted, dashed).
    pub fn write_box(&mut self, id: &str, tip: &str, r: Rect, size: f32) {
        let (x, y, w, h) = r;
        let mut tf = TextField::new("", size);
        tf.style = Style::SansBold;
        tf.color = self.pal.ink;
        tf.align = TextAlign::Center;
        let f = FormField::new(id, x, y, w, h, FieldKind::Text(tf))
            .with_tooltip(tip)
            .with_frame(self.frame_write());
        self.ops.push(PageOp::Field(f));
    }

    /// A multi-line prefilled field; returns its height. `max_lines` caps the height; a longer
    /// value is cut with an ellipsis (values are printed, so nothing may run past the box).
    pub fn multiline_field(
        &mut self,
        id: &str,
        tip: &str,
        (x, y, w): (f32, f32, f32),
        value: &str,
        t: &Txt,
        max_lines: usize,
    ) -> f32 {
        let width = (w - 2.0 * FIELD_PAD).max(1.0);
        let value = cut_lines(&typographic_minus(value), t.style, t.size, width, max_lines);
        let leading = t.size * 1.3;
        let n = value.lines().count().clamp(1, max_lines.max(1));
        let h = leading * n as f32 + 1.0;
        let mut tf = TextField::new(&value, t.size);
        tf.style = t.style;
        tf.color = t.color;
        tf.align = t.align.into();
        tf.multiline = true;
        tf.leading = Some(leading);
        tf.pad = FIELD_PAD;
        tf.baseline = Some(BASELINE * t.size + 0.4);
        let f = FormField::new(id, x, y, w, h, FieldKind::Text(tf)).with_tooltip(tip);
        self.ops.push(PageOp::Field(f));
        h
    }

    /// A check box. `framed` draws the outline as page content.
    #[allow(clippy::too_many_arguments)]
    pub fn check(
        &mut self,
        id: &str,
        tip: &str,
        (x, y, size): (f32, f32, f32),
        shape: Shape,
        mark: Mark,
        checked: bool,
        framed: bool,
    ) {
        let mut cf = CheckField::new(checked);
        cf.mark = mark;
        cf.shape = shape;
        cf.color = if mark == Mark::Fill {
            self.pal.gold
        } else {
            self.pal.ink
        };
        let frame = FieldFrame {
            shape,
            paint: Paint::stroke(self.pal.ink, 0.7),
            radius: 1.2,
        };
        let mut f = FormField::new(id, x, y, size, size, FieldKind::Check(cf)).with_tooltip(tip);
        if framed {
            f = f.with_frame(frame);
        }
        self.ops.push(PageOp::Field(f));
    }
}
