//! AcroForm fields: text fields and check boxes with appearance streams drawn in the embedded
//! Noto subsets, a hierarchical field tree built from dotted names (`abil.str.score`), and the
//! flatten mode that draws the same appearances into the page with no widgets.
//!
//! Coordinates follow the page model of [`crate::pdf`]: top-left origin, y down, points. The
//! optional [`FieldFrame`] (box, circle or diamond outline and tint) is page content in both
//! modes, so a flattened page equals the fillable page plus the field values.
//!
//! Viewer compatibility: poppler, pdfium and Acrobat draw a widget's `/AP`, but Apple PDFKit
//! (Preview, Safari, iOS) ignores `/AP` for text widgets and redraws `/V` itself, in the `/DA`
//! font looked up among installed fonts only (an embedded Noto subset, Type0 or simple, falls
//! back to plain Helvetica), with its own insets, so prefilled values lost their design font and
//! wrapped or clipped out of sight. So a prefilled value is printed as page content in both modes
//! (vector Noto, the same in every viewer) and only empty play fields and check boxes are
//! widgets. `/DA` names a standard-14 face of the field's weight and family, which every viewer
//! (PDFKit included) can draw what a player types in.

use crate::fonts::{text_width, Face, Style};
use crate::pdf::{pdf_string, show_text, FormMode, PageOp, PdfPage, TextSpec, Writer, BASELINE};
use crate::pdf_draw::{color, paint_path, Paint, Path, Rgb, Seg};
use crate::pdf_fonts::{EmbeddedFonts, Usage};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Field flag bits (PDF 32000-1 tables 221, 228).
const FF_READ_ONLY: u32 = 1;
const FF_MULTILINE: u32 = 1 << 12;
const FF_DO_NOT_SPELL_CHECK: u32 = 1 << 22;
/// Annotation flag: print.
const ANNOT_PRINT: u32 = 4;
/// Appearance state name of a ticked check box.
pub(crate) const ON_STATE: &str = "Yes";
/// Default inner padding of a text field, in points.
const DEFAULT_PAD: f32 = 2.0;
/// Default line height of a multi-line field, in em.
const DEFAULT_LEADING: f32 = 1.25;
/// Cap height used when a face has none, in em.
const FALLBACK_CAP: f32 = 0.714;
/// Inset of a filled check mark, as a fraction of the smaller side.
const MARK_INSET: f32 = 0.22;
/// Stroke width of tick and cross marks, as a fraction of the smaller side.
const MARK_STROKE: f32 = 0.13;
const MIN_MARK_STROKE: f32 = 0.6;
/// Print ink of the sheet palette (#1F272B).
pub(crate) const FIELD_INK: Rgb = (0.122, 0.153, 0.169);

/// Horizontal alignment (`/Q`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

impl TextAlign {
    fn quad(self) -> u8 {
        match self {
            TextAlign::Start => 0,
            TextAlign::Center => 1,
            TextAlign::End => 2,
        }
    }
}

/// A text field (prefilled value or an empty play box).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextField {
    pub value: String,
    pub style: Style,
    pub size: f32,
    pub color: Rgb,
    pub align: TextAlign,
    /// Wrap to the field width and accept new lines.
    pub multiline: bool,
    /// Line height in points (default `1.25 size`).
    pub leading: Option<f32>,
    /// Horizontal inner padding in points.
    pub pad: f32,
    /// First baseline, in points from the top of the field. Default: cap height centred for a
    /// single line, `pad + 0.8 size` for multi-line.
    pub baseline: Option<f32>,
    pub read_only: bool,
}

impl TextField {
    pub fn new(value: &str, size: f32) -> TextField {
        TextField {
            value: value.to_string(),
            style: Style::Sans,
            size,
            color: FIELD_INK,
            align: TextAlign::Start,
            multiline: false,
            leading: None,
            pad: DEFAULT_PAD,
            baseline: None,
            read_only: false,
        }
    }

    fn leading(&self) -> f32 {
        self.leading.unwrap_or(self.size * DEFAULT_LEADING)
    }
}

/// The mark drawn when a check box is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mark {
    #[default]
    Tick,
    Cross,
    /// The field shape, inset and filled (pips, diamonds, dots).
    Fill,
}

/// Outline shape of a box or a filled mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Shape {
    #[default]
    Square,
    Circle,
    Diamond,
}

/// A two-state check box (`/Yes` and `/Off` appearances).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CheckField {
    pub checked: bool,
    pub mark: Mark,
    pub shape: Shape,
    pub color: Rgb,
    pub read_only: bool,
}

impl CheckField {
    pub fn new(checked: bool) -> CheckField {
        CheckField {
            checked,
            mark: Mark::Tick,
            shape: Shape::Square,
            color: FIELD_INK,
            read_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FieldKind {
    Text(TextField),
    Check(CheckField),
}

/// Outline and tint drawn as page content under the field (both modes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FieldFrame {
    pub shape: Shape,
    pub paint: Paint,
    /// Corner radius of a square frame.
    pub radius: f32,
}

/// One form field placed on a page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FormField {
    /// Fully qualified name; dots make a hierarchy (`abil.str.score`). Made unique per document.
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub kind: FieldKind,
    /// `/TU` (accessible name, tooltip).
    pub tooltip: Option<String>,
    pub frame: Option<FieldFrame>,
    /// `/StructParent` key of the widget (tagging hook).
    pub struct_parent: Option<u32>,
}

impl FormField {
    pub fn new(name: &str, x: f32, y: f32, w: f32, h: f32, kind: FieldKind) -> FormField {
        FormField {
            name: name.to_string(),
            x,
            y,
            w,
            h,
            kind,
            tooltip: None,
            frame: None,
            struct_parent: None,
        }
    }

    #[must_use]
    pub fn with_tooltip(self, tooltip: &str) -> FormField {
        FormField {
            tooltip: Some(tooltip.to_string()),
            ..self
        }
    }

    #[must_use]
    pub fn with_frame(self, frame: FieldFrame) -> FormField {
        FormField {
            frame: Some(frame),
            ..self
        }
    }
}

/// `shape` inscribed in the box, in y-down coordinates.
fn shape_path(shape: Shape, (x, y, w, h): (f32, f32, f32, f32), radius: f32) -> Path {
    match shape {
        Shape::Square => Path::rounded_rect(x, y, w, h, radius),
        Shape::Circle => Path::ellipse(x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0),
        Shape::Diamond => Path::diamond(x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0),
    }
}

/// Greedy word wrap to `max` points (hard-breaks words wider than a line).
pub(crate) fn wrap(text: &str, style: Style, size: f32, max: f32) -> Vec<String> {
    let fits = |s: &str| text_width(style, s, size) <= max;
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if fits(&candidate) {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for c in word.chars() {
                let mut next = line.clone();
                next.push(c);
                if !line.is_empty() && !fits(&next) {
                    lines.push(std::mem::replace(&mut line, c.to_string()));
                } else {
                    line = next;
                }
            }
        }
        lines.push(line);
    }
    lines
}

fn cap_height(face: Face) -> f32 {
    let parsed = face.parsed();
    parsed
        .capital_height()
        .map_or(FALLBACK_CAP, |c| f32::from(c) / face.units_per_em())
}

/// One laid-out line of a field value: left edge and baseline in field-local points from the
/// top-left corner, and the text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValueLine {
    pub x: f32,
    pub base: f32,
    pub text: String,
}

/// Descender allowance below a baseline, in em: a line is drawn only when it fits whole.
const LINE_DESCENT: f32 = 0.2;
/// Rounding slack when fitting lines, in points.
const FIT_EPS: f32 = 0.3;

/// How many lines, the first baseline `first` points below the top and then every `leading`,
/// fit whole (descenders included) in a box `h` points tall.
pub(crate) fn fitting_lines(first: f32, leading: f32, size: f32, h: f32) -> usize {
    let room = h + FIT_EPS - LINE_DESCENT * size - first;
    if room < 0.0 {
        0
    } else {
        (room / leading.max(0.1)).floor() as usize + 1
    }
}

/// The lines a field value draws, plus how many wrapped lines fall below the box (not drawn).
pub(crate) fn value_lines(t: &TextField, w: f32, h: f32) -> (Vec<ValueLine>, usize) {
    let lines = if t.multiline {
        wrap(&t.value, t.style, t.size, (w - 2.0 * t.pad).max(1.0))
    } else {
        vec![t.value.replace('\n', " ")]
    };
    let first = t.baseline.unwrap_or_else(|| {
        if t.multiline {
            t.pad + BASELINE * t.size
        } else {
            (h + cap_height(t.style.primary()) * t.size) / 2.0
        }
    });
    let total = lines.len();
    // A single line always shows (its box is sized for it); further lines only when whole.
    let fit = fitting_lines(first, t.leading(), t.size, h).max(1);
    let shown: Vec<ValueLine> = lines
        .into_iter()
        .take(fit)
        .enumerate()
        .map(|(i, text)| (first + i as f32 * t.leading(), text))
        .map(|(base, text)| {
            let tw = text_width(t.style, &text, t.size);
            let x = match t.align {
                TextAlign::Start => t.pad,
                TextAlign::Center => ((w - tw) / 2.0).max(t.pad),
                TextAlign::End => (w - t.pad - tw).max(t.pad),
            };
            ValueLine { x, base, text }
        })
        .collect();
    let dropped = total - shown.len();
    (shown, dropped)
}

/// Text drawing operators in field-local PDF space (origin bottom-left, y up), clipped.
fn text_body(t: &TextField, w: f32, h: f32, fonts: &EmbeddedFonts) -> String {
    let mut s = format!("q 0 0 {w:.2} {h:.2} re W n\n");
    let spec = TextSpec {
        style: t.style,
        size: t.size,
        color: t.color,
    };
    for line in value_lines(t, w, h).0 {
        show_text(&mut s, fonts, spec, line.x, h - line.base, &line.text);
    }
    s.push_str("Q\n");
    s
}

/// The on-state mark of a check box, in field-local PDF space.
fn mark_body(c: &CheckField, w: f32, h: f32) -> String {
    let side = w.min(h);
    let stroke = Paint::stroke(c.color, (side * MARK_STROKE).max(MIN_MARK_STROKE)).rounded();
    let path = match c.mark {
        Mark::Tick => Path::polyline(&[
            (0.22 * w, 0.52 * h),
            (0.42 * w, 0.72 * h),
            (0.80 * w, 0.28 * h),
        ]),
        Mark::Cross => Path::new(vec![
            Seg::Move(0.27 * w, 0.27 * h),
            Seg::Line(0.73 * w, 0.73 * h),
            Seg::Move(0.73 * w, 0.27 * h),
            Seg::Line(0.27 * w, 0.73 * h),
        ]),
        Mark::Fill => {
            let i = side * MARK_INSET;
            let path = shape_path(c.shape, (i, i, w - 2.0 * i, h - 2.0 * i), i / 2.0);
            let mut s = String::new();
            paint_path(&mut s, &path, &Paint::fill(c.color), h);
            return s;
        }
    };
    let mut s = String::new();
    paint_path(&mut s, &path, &stroke, h);
    s
}

/// Record the glyphs a field's appearance draws.
pub(crate) fn record_usage(field: &FormField, usage: &mut Usage) {
    if let FieldKind::Text(t) = &field.kind {
        usage.record_text(t.style, &t.value);
    }
}

/// `/DR` resource name and base font of the standard-14 face a style's `/DA` names.
fn std_font(style: Style) -> (&'static str, &'static str) {
    match style {
        Style::Sans => ("Helv", "Helvetica"),
        Style::SansBold => ("HeBo", "Helvetica-Bold"),
        Style::SansItalic => ("HeOb", "Helvetica-Oblique"),
        Style::Serif => ("TiRo", "Times-Roman"),
        Style::SerifBold => ("TiBo", "Times-Bold"),
        Style::SerifItalic => ("TiIt", "Times-Italic"),
    }
}

/// Every style, in `/DR` order.
const STYLES: [Style; 6] = [
    Style::Sans,
    Style::SansBold,
    Style::SansItalic,
    Style::Serif,
    Style::SerifBold,
    Style::SerifItalic,
];

/// A field can show more than one line (otherwise it is written as single-line, which PDFKit
/// centres vertically instead of top-aligning and clipping what a player types).
fn wraps(t: &TextField, h: f32) -> bool {
    t.multiline && h >= 2.0 * t.leading()
}

/// The field becomes a widget when fillable: a check box or an empty text field (a prefilled
/// value is printed instead, see the module notes).
pub(crate) fn is_widget(field: &FormField) -> bool {
    match &field.kind {
        FieldKind::Text(t) => t.value.trim().is_empty(),
        FieldKind::Check(_) => true,
    }
}

/// Page content for a field: its frame, plus its value unless it is a fillable widget.
pub(crate) fn page_ops(
    field: &FormField,
    fonts: &EmbeddedFonts,
    page_h: f32,
    mode: FormMode,
) -> String {
    let mut s = String::new();
    let rect = (field.x, field.y, field.w, field.h);
    if let Some(frame) = &field.frame {
        let path = shape_path(frame.shape, rect, frame.radius);
        paint_path(&mut s, &path, &frame.paint, page_h);
    }
    if mode == FormMode::Fillable && is_widget(field) {
        return s;
    }
    let body = match &field.kind {
        FieldKind::Text(t) => text_body(t, field.w, field.h, fonts),
        FieldKind::Check(c) if c.checked => mark_body(c, field.w, field.h),
        FieldKind::Check(_) => return s,
    };
    let _ = write!(
        s,
        "q 1 0 0 1 {:.2} {:.2} cm\n{body}Q\n",
        field.x,
        page_h - field.y - field.h
    );
    s
}

// ---------------------------------------------------------------------------------------------
// Field tree
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kid {
    Node(usize),
    Leaf(usize),
}

#[derive(Debug)]
struct Node {
    partial: String,
    parent: Option<usize>,
    kids: Vec<Kid>,
}

#[derive(Debug)]
struct Leaf {
    partial: String,
    parent: Option<usize>,
}

/// Non-terminal fields (`abil`, `abil.str`) and terminal widgets, in document order.
#[derive(Debug, Default)]
struct FieldTree {
    nodes: Vec<Node>,
    leaves: Vec<Leaf>,
    roots: Vec<Kid>,
}

fn segments(name: &str) -> Vec<String> {
    let name = name.trim();
    if name.is_empty() {
        return vec!["field".to_string()];
    }
    name.split('.')
        .map(|s| {
            let s = s.trim();
            if s.is_empty() {
                "_".to_string()
            } else {
                s.to_string()
            }
        })
        .collect()
}

impl FieldTree {
    /// Insert names in order. A duplicate gets `@2`, `@3`... on its last part; a name under an
    /// existing terminal field (`ac.basis` after `ac`) is flattened to `ac_basis`.
    fn build<'a>(names: impl IntoIterator<Item = &'a str>) -> FieldTree {
        let mut tree = FieldTree::default();
        let mut node_at: HashMap<String, usize> = HashMap::new();
        let mut leaf_names: HashSet<String> = HashSet::new();
        for name in names {
            let mut segs = segments(name);
            let under_leaf = (1..segs.len()).any(|k| leaf_names.contains(&segs[..k].join(".")));
            if under_leaf {
                segs = vec![segs.join("_")];
            }
            let last = segs.last().cloned().unwrap_or_default();
            let mut n = 1;
            while {
                let full = segs.join(".");
                leaf_names.contains(&full) || node_at.contains_key(&full)
            } {
                n += 1;
                if let Some(l) = segs.last_mut() {
                    *l = format!("{last}@{n}");
                }
            }
            if n > 1 || under_leaf {
                tracing::warn!("form field name {name:?} renamed to {:?}", segs.join("."));
            }
            let mut parent = None;
            for i in 0..segs.len() - 1 {
                let path = segs[..=i].join(".");
                let idx = match node_at.get(&path) {
                    Some(&idx) => idx,
                    None => {
                        let idx = tree.nodes.len();
                        tree.nodes.push(Node {
                            partial: segs[i].clone(),
                            parent,
                            kids: Vec::new(),
                        });
                        tree.attach(parent, Kid::Node(idx));
                        node_at.insert(path, idx);
                        idx
                    }
                };
                parent = Some(idx);
            }
            let leaf = tree.leaves.len();
            tree.leaves.push(Leaf {
                partial: segs[segs.len() - 1].clone(),
                parent,
            });
            tree.attach(parent, Kid::Leaf(leaf));
            leaf_names.insert(segs.join("."));
        }
        tree
    }

    fn attach(&mut self, parent: Option<usize>, kid: Kid) {
        match parent {
            Some(p) => self.nodes[p].kids.push(kid),
            None => self.roots.push(kid),
        }
    }

    /// Fully qualified name of a terminal field.
    #[cfg(test)]
    fn full_name(&self, leaf: usize) -> String {
        let mut parts = vec![self.leaves[leaf].partial.clone()];
        let mut at = self.leaves[leaf].parent;
        while let Some(p) = at {
            parts.push(self.nodes[p].partial.clone());
            at = self.nodes[p].parent;
        }
        parts.reverse();
        parts.join(".")
    }
}

// ---------------------------------------------------------------------------------------------
// Widget serialisation
// ---------------------------------------------------------------------------------------------

/// Writes widgets page by page, then the non-terminal fields and the `/AcroForm` dictionary.
pub(crate) struct FormWriter {
    tree: FieldTree,
    node_ids: Vec<usize>,
    leaf_ids: Vec<usize>,
    /// Styles named by a `/DA` (their standard-14 fonts go into `/DR`).
    da_styles: HashSet<Style>,
}

/// The fields of a page that are written as widgets.
fn fields(page: &PdfPage) -> impl Iterator<Item = &FormField> {
    page.ops.iter().filter_map(|op| match op {
        PageOp::Field(f) if is_widget(f) => Some(f),
        _ => None,
    })
}

impl FormWriter {
    /// Build the field tree for every field of the document and reserve the node objects.
    pub fn plan(pages: &[PdfPage], w: &mut Writer) -> FormWriter {
        let tree = FieldTree::build(pages.iter().flat_map(fields).map(|f| f.name.as_str()));
        let node_ids = tree.nodes.iter().map(|_| w.alloc(1)).collect();
        FormWriter {
            tree,
            node_ids,
            leaf_ids: Vec::new(),
            da_styles: HashSet::new(),
        }
    }

    /// Write the widgets of one page; returns their object ids for `/Annots`.
    pub fn write_page_widgets(
        &mut self,
        w: &mut Writer,
        page: &PdfPage,
        page_obj: usize,
        page_h: f32,
        fonts: &EmbeddedFonts,
        font_res: &str,
    ) -> Vec<usize> {
        let mut ids = Vec::new();
        for field in fields(page) {
            let leaf = self.leaf_ids.len();
            let id = w.alloc(1);
            self.leaf_ids.push(id);
            ids.push(id);
            let mut dict = format!(
                "<< /Type /Annot /Subtype /Widget /F {ANNOT_PRINT} /P {page_obj} 0 R \
                 /Rect [ {:.2} {:.2} {:.2} {:.2} ]",
                field.x,
                page_h - field.y - field.h,
                field.x + field.w,
                page_h - field.y
            );
            if let Some(p) = self.tree.leaves.get(leaf).and_then(|l| l.parent) {
                let _ = write!(dict, " /Parent {} 0 R", self.node_ids[p]);
            }
            let partial = self
                .tree
                .leaves
                .get(leaf)
                .map_or(field.name.as_str(), |l| l.partial.as_str());
            let _ = write!(dict, " /T {}", pdf_string(partial));
            if let Some(tu) = &field.tooltip {
                let _ = write!(dict, " /TU {}", pdf_string(tu));
            }
            if let Some(sp) = field.struct_parent {
                let _ = write!(dict, " /StructParent {sp}");
            }
            let bbox = format!("/BBox [ 0 0 {:.2} {:.2} ]", field.w, field.h);
            match &field.kind {
                FieldKind::Text(t) => {
                    let ap = w.alloc(1);
                    let mut ff = FF_DO_NOT_SPELL_CHECK;
                    if wraps(t, field.h) {
                        ff |= FF_MULTILINE;
                    }
                    if t.read_only {
                        ff |= FF_READ_ONLY;
                    }
                    let _ = write!(
                        dict,
                        " /FT /Tx /Ff {ff} /V {} /DA (/{} {:.2} Tf {} rg) /Q {} /AP << /N {ap} 0 R >> >>",
                        pdf_string(&t.value),
                        std_font(t.style).0,
                        t.size,
                        color(t.color),
                        t.align.quad()
                    );
                    self.da_styles.insert(t.style);
                    let body = format!("/Tx BMC\n{}EMC", text_body(t, field.w, field.h, fonts));
                    w.stream(
                        ap,
                        &format!(
                            "/Type /XObject /Subtype /Form {bbox} /Resources << /Font << {font_res}>> >>"
                        ),
                        body.as_bytes(),
                    );
                }
                FieldKind::Check(c) => {
                    let (on, off) = (w.alloc(1), w.alloc(1));
                    let state = if c.checked { ON_STATE } else { "Off" };
                    let ff = if c.read_only { FF_READ_ONLY } else { 0 };
                    let _ = write!(
                        dict,
                        " /FT /Btn /Ff {ff} /V /{state} /AS /{state} \
                         /AP << /N << /{ON_STATE} {on} 0 R /Off {off} 0 R >> >> >>"
                    );
                    let xobj = format!("/Type /XObject /Subtype /Form {bbox} /Resources << >>");
                    w.stream(on, &xobj, mark_body(c, field.w, field.h).as_bytes());
                    w.stream(off, &xobj, b"");
                }
            }
            w.object(id, dict.as_bytes());
        }
        ids
    }

    fn kid_ref(&self, kid: Kid) -> String {
        match kid {
            Kid::Node(n) => format!("{} 0 R ", self.node_ids[n]),
            Kid::Leaf(l) => format!("{} 0 R ", self.leaf_ids[l]),
        }
    }

    /// Write the non-terminal fields and the `/AcroForm` dictionary (`None` without fields).
    pub fn finish(self, w: &mut Writer) -> Option<usize> {
        if self.leaf_ids.is_empty() {
            return None;
        }
        for (node, &id) in self.tree.nodes.iter().zip(&self.node_ids) {
            let kids: String = node.kids.iter().map(|k| self.kid_ref(*k)).collect();
            let mut dict = format!("<< /T {} /Kids [ {kids}]", pdf_string(&node.partial));
            if let Some(p) = node.parent {
                let _ = write!(dict, " /Parent {} 0 R", self.node_ids[p]);
            }
            dict.push_str(" >>");
            w.object(id, dict.as_bytes());
        }
        let roots: String = self.tree.roots.iter().map(|k| self.kid_ref(*k)).collect();
        let mut dr = String::new();
        for style in STYLES
            .into_iter()
            .filter(|s| *s == Style::Sans || self.da_styles.contains(s))
        {
            let (name, base) = std_font(style);
            let id = w.alloc(1);
            w.object(
                id,
                format!(
                    "<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>"
                )
                .as_bytes(),
            );
            let _ = write!(dr, "/{name} {id} 0 R ");
        }
        let id = w.alloc(1);
        w.object(
            id,
            format!(
                "<< /Fields [ {roots}] /DR << /Font << {dr}>> >> /DA (/{} 0 Tf 0 g) >>",
                std_font(Style::Sans).0
            )
            .as_bytes(),
        );
        Some(id)
    }
}

#[cfg(test)]
#[path = "pdf_form_tests.rs"]
mod tests;
