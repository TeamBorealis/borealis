//! PDF 1.4 writer: embedded subset TrueType fonts (Type0/Identity-H with `ToUnicode`), vector
//! paths (fill, stroke, clip, ExtGState alpha), JPEG and PNG portraits, and AcroForm fields
//! ([`crate::pdf_form`]). Pure Rust, no system dependencies, byte-for-byte deterministic (no
//! timestamps, no document id).
//!
//! Layout code builds a [`PdfDocument`] of [`PageOp`]s in a top-left, y-down page space (points)
//! and calls [`write_document`]. Tagging hooks in through [`PageOp::BeginTag`],
//! [`PageOp::BeginArtifact`], [`PageOp::EndMarked`], `struct_parents` and [`StructureWriter`].

use crate::art::{ArtPath, ArtPiece, Op};
use crate::fonts::{Face, Style};
use crate::image::{PdfImage, Samples};
use crate::pdf_draw::{
    alpha_key, color, gs_name, paint_path, path_ops, Paint, Path, Rgb, Seg, ALPHA_STEPS,
};
use crate::pdf_fonts::{EmbeddedFont, EmbeddedFonts, Usage};
use crate::pdf_form::{self, FormField};
use miniz_oxide::deflate::compress_to_vec_zlib;
use std::collections::BTreeSet;
use std::fmt::Write as _;

const CATALOG_OBJ: usize = 1;
const PAGES_OBJ: usize = 2;
const INFO_OBJ: usize = 3;
const FIRST_FREE_OBJ: usize = 4;
/// Distance from the top of a line box to its baseline, in em.
pub(crate) const BASELINE: f32 = 0.8;
/// zlib level for streams (deterministic for a given input).
const DEFLATE_LEVEL: u8 = 6;

// ---------------------------------------------------------------------------------------------
// Document model
// ---------------------------------------------------------------------------------------------

/// A positioned drawing operation in page space (top-left origin, y down, points).
#[derive(Debug, Clone)]
pub(crate) enum PageOp {
    /// One line of text; `y` is the top of the line box (baseline at `y + 0.8 size`).
    Text {
        x: f32,
        y: f32,
        size: f32,
        style: Style,
        color: Rgb,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        width: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Rgb,
        width: f32,
    },
    Path {
        path: Path,
        paint: Paint,
    },
    /// Draw image `index` of [`PdfDocument::images`] into the box.
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        index: usize,
        /// Alt text of the tagged figure.
        alt: &'static str,
    },
    /// An art piece drawn through a shared Form XObject (one per piece per document). Piece
    /// units map to page space with `(sx, sy)` points per unit (y down) and origin `(x, y)`.
    /// Only valid when the placement keeps stroke widths undistorted (uniform scale, or a piece
    /// without strokes); see [`art_form_ok`].
    Art {
        piece: &'static ArtPiece,
        x: f32,
        y: f32,
        sx: f32,
        sy: f32,
    },
    /// A form field: a widget (fillable) or its appearance drawn into the page (flatten).
    Field(FormField),
    /// Save the graphics state (`q`); pair with [`PageOp::Restore`].
    Save,
    /// Restore the graphics state (`Q`).
    Restore,
    /// Intersect the clip with `path` until the next [`PageOp::Restore`].
    Clip {
        path: Path,
        even_odd: bool,
    },
    /// Fill and stroke opacity for everything until the next [`PageOp::Restore`].
    #[cfg_attr(not(test), allow(dead_code))] // the layout uses per-path opacity
    Opacity(f32),
    /// Begin a marked-content sequence: `/tag <</MCID n>> BDC` (tagging hook).
    BeginTag {
        tag: String,
        mcid: Option<u32>,
    },
    /// Structure marker: open a grouping element (`Table`, `TR`, `TD`, `L`, `LI`...). Consumed
    /// by [`crate::layout::tag_pages`]; the writer ignores it.
    Group(&'static str),
    /// Structure marker: close the innermost [`PageOp::Group`].
    GroupEnd,
    /// Structure marker: begin a content item of `tag` (optionally with `/ActualText`), closed by
    /// [`PageOp::EndMarked`]. Tagging turns it into [`PageOp::BeginTag`] with an MCID; an
    /// untagged document writes a plain `/Span BMC`.
    Leaf {
        tag: &'static str,
        actual: Option<String>,
    },
    /// Begin an `/Artifact` sequence (ornaments, rules, page furniture).
    BeginArtifact,
    /// End the innermost [`PageOp::BeginTag`] or [`PageOp::BeginArtifact`] (`EMC`).
    EndMarked,
}

/// Whether form fields stay editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FormMode {
    /// AcroForm widgets with appearance streams in the embedded fonts.
    #[default]
    Fillable,
    /// The same appearances drawn into the page content; no AcroForm, no widgets.
    Flatten,
}

/// One page of a [`PdfDocument`].
#[derive(Debug, Clone, Default)]
pub(crate) struct PdfPage {
    pub ops: Vec<PageOp>,
    /// `/StructParents` key of the page (tagging hook).
    pub struct_parents: Option<u32>,
}

/// Objects written by the tagging hook after pages and widgets have ids.
pub(crate) struct DocRefs {
    /// Page object ids, in page order.
    pub pages: Vec<usize>,
    /// Widget annotation object ids per page, in [`PageOp::Field`] order.
    pub widgets: Vec<Vec<usize>>,
}

/// Lets a tagging pass add objects (structure tree, parent tree) and catalog entries.
pub(crate) trait StructureWriter {
    /// Write extra objects through `sink`; return extra catalog entries (for example
    /// `/StructTreeRoot 40 0 R /MarkInfo << /Marked true >>`).
    fn write(&self, sink: &mut ObjectSink<'_>, refs: &DocRefs) -> String;
}

/// A document ready to serialise.
pub(crate) struct PdfDocument<'a> {
    pub page_w: f32,
    pub page_h: f32,
    pub pages: Vec<PdfPage>,
    pub images: Vec<PdfImage>,
    pub title: String,
    /// BCP 47 language of the content (`/Lang`).
    pub lang: Option<String>,
    pub mode: FormMode,
    pub structure: Option<&'a dyn StructureWriter>,
}

// ---------------------------------------------------------------------------------------------
// Serialisation
// ---------------------------------------------------------------------------------------------

/// Escape a string as a UTF-16BE hex string with BOM (`<FEFF....>`).
pub(crate) fn utf16_hex_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for u in text.encode_utf16() {
        let _ = write!(out, "{u:04X}");
    }
    out.push('>');
    out
}

/// A PDF string: a literal for printable ASCII, otherwise UTF-16BE hex.
pub(crate) fn pdf_string(text: &str) -> String {
    if !text.chars().all(|c| (' '..='~').contains(&c)) {
        return utf16_hex_string(text);
    }
    let mut out = String::from("(");
    for c in text.chars() {
        if matches!(c, '(' | ')' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push(')');
    out
}

/// 16-byte file identifier of `body` as hex: two FNV-1a 64-bit hashes with different offsets.
fn file_id(body: &[u8]) -> String {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let hash = |seed: u64| {
        body.iter()
            .fold(seed, |h, b| (h ^ u64::from(*b)).wrapping_mul(PRIME))
    };
    format!(
        "{:016x}{:016x}",
        hash(0xcbf2_9ce4_8422_2325),
        hash(0x8422_2325_cbf2_9ce4)
    )
}

/// Grid (points) that thin horizontal rules snap to: one device pixel at 150 dpi (two at 300).
const RULE_GRID: f32 = 72.0 / 150.0;
/// Rules at most this wide snap (hairline rules of ruled fields and tables).
const RULE_SNAP_MAX_W: f32 = 0.6;

/// PDF-space y of a line's ends (`top` is the line's y from the page top): a thin horizontal
/// rule is centred on a 150 dpi pixel centre (B-21 L5: at 150/300 dpi a 0.5 pt rule rasterised
/// 1-2 / 2-3 px thick depending on its sub-pixel y, so evenly pitched rules looked doubled at
/// random). Other lines are unchanged.
fn snap_rule(h: f32, (top_a, top_b): (f32, f32), width: f32) -> (f32, f32) {
    if (top_a - top_b).abs() > f32::EPSILON || width > RULE_SNAP_MAX_W {
        return (h - top_a, h - top_b);
    }
    let y = ((top_a / RULE_GRID - 0.5).round() + 0.5) * RULE_GRID;
    (h - y, h - y)
}

#[cfg(test)]
mod rule_snap_tests {
    use super::{snap_rule, RULE_GRID};

    #[test]
    fn thin_horizontal_rules_sit_on_150_dpi_pixel_centres() {
        for i in 0..200 {
            let top = 100.0 + i as f32 * 0.137;
            let (a, b) = snap_rule(792.0, (top, top), 0.5);
            assert_eq!(a, b);
            let px = (792.0 - a) / RULE_GRID;
            assert!((px - px.floor() - 0.5).abs() < 0.01, "{top}: {px}");
            assert!((792.0 - a - top).abs() <= RULE_GRID / 2.0 + 0.01);
        }
        // Thick and slanted lines keep their position.
        assert_eq!(snap_rule(792.0, (100.3, 100.3), 0.9), (691.7, 691.7));
        assert_eq!(snap_rule(792.0, (100.3, 120.0), 0.5), (691.7, 672.0));
    }
}

fn art_seg(op: Op) -> Seg {
    match op {
        Op::Move(x, y) => Seg::Move(x, y),
        Op::Line(x, y) => Seg::Line(x, y),
        Op::Cubic(a, b, c, d, e, f) => Seg::Cubic(a, b, c, d, e, f),
        Op::Close => Seg::Close,
    }
}

fn art_rgb(c: [u8; 3]) -> Rgb {
    (
        f32::from(c[0]) / 255.0,
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
    )
}

/// The path and paint of one art path in piece units; stroke widths are multiplied by
/// `line_scale` (points per unit).
pub(crate) fn art_path_paint(p: &ArtPath, line_scale: f32) -> (Path, Paint) {
    let path = Path::new(p.ops().map(art_seg).collect());
    let mut paint = Paint {
        fill: p.fill.map(art_rgb),
        ..Paint::default()
    };
    if let Some(s) = p.stroke {
        paint = paint
            .with_stroke(art_rgb(s.rgb), s.width * line_scale)
            .rounded();
    }
    (path, paint.with_opacity(p.alpha))
}

/// Whether placing `piece` at `(sx, sy)` through a Form XObject draws exactly what the direct
/// paths would (a non-uniform scale distorts stroke widths).
pub(crate) fn art_form_ok(piece: &ArtPiece, sx: f32, sy: f32) -> bool {
    (sx.abs() - sy.abs()).abs() < 1e-4 || piece.paths.iter().all(|p| p.stroke.is_none())
}

/// Content stream of the Form XObject of `piece` (piece units, y flipped to PDF up).
fn art_form_content(piece: &ArtPiece) -> String {
    let mut s = String::new();
    for p in piece.paths {
        let (path, paint) = art_path_paint(p, 1.0);
        paint_path(&mut s, &path, &paint, 0.0);
    }
    s
}

/// Pieces used by `ops`, in first-use order, appended to `seen` (identity by address).
fn collect_art(ops: &[PageOp], seen: &mut Vec<&'static ArtPiece>) {
    for op in ops {
        if let PageOp::Art { piece, .. } = op {
            if !seen.iter().any(|q| std::ptr::eq(*q, *piece)) {
                seen.push(piece);
            }
        }
    }
}

/// Font, size and colour of a line of text.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextSpec {
    pub style: Style,
    pub size: f32,
    pub color: Rgb,
}

/// Show one line of text with its baseline at PDF `(x, y)` (y up). Bidi-reordered lines carry
/// `/ActualText` so search and copy stay in reading order.
pub(crate) fn show_text(
    out: &mut String,
    fonts: &EmbeddedFonts,
    spec: TextSpec,
    x: f32,
    y: f32,
    text: &str,
) {
    let Some(enc) = fonts.encode(spec.style, text, spec.size) else {
        return;
    };
    if enc.reordered {
        let _ = writeln!(
            out,
            "/Span << /ActualText {} >> BDC",
            utf16_hex_string(text)
        );
    }
    let _ = writeln!(
        out,
        "BT {} rg 1 0 0 1 {x:.2} {y:.2} Tm {} ET",
        color(spec.color),
        enc.ops
    );
    if enc.reordered {
        out.push_str(
            "EMC
",
        );
    }
}

struct PageCtx<'a> {
    art: &'a [&'static ArtPiece],
    page_h: f32,
    fonts: &'a EmbeddedFonts,
    mode: FormMode,
}

fn page_content(ops: &[PageOp], cx: &PageCtx<'_>) -> String {
    let h = cx.page_h;
    let mut s = String::new();
    for op in ops {
        match op {
            PageOp::Text {
                x,
                y,
                size,
                style,
                color: c,
                text,
            } => show_text(
                &mut s,
                cx.fonts,
                TextSpec {
                    style: *style,
                    size: *size,
                    color: *c,
                },
                *x,
                h - (y + size * BASELINE),
                text,
            ),
            PageOp::Rect {
                x,
                y,
                w,
                h: rh,
                fill,
                stroke,
                width,
            } => {
                let py = h - y - rh;
                if let Some(f) = fill {
                    let _ = writeln!(s, "{} rg {x:.2} {py:.2} {w:.2} {rh:.2} re f", color(*f));
                }
                if let Some(st) = stroke {
                    let _ = writeln!(
                        s,
                        "{} RG {width:.2} w {x:.2} {py:.2} {w:.2} {rh:.2} re S",
                        color(*st)
                    );
                }
            }
            PageOp::Line {
                x1,
                y1,
                x2,
                y2,
                color: c,
                width,
            } => {
                let (ya, yb) = snap_rule(h, (*y1, *y2), *width);
                let _ = writeln!(
                    s,
                    "{} RG {width:.2} w {x1:.2} {ya:.2} m {x2:.2} {yb:.2} l S",
                    color(*c),
                );
            }
            PageOp::Path { path, paint } => paint_path(&mut s, path, paint, h),
            PageOp::Image {
                x,
                y,
                w,
                h: ih,
                index,
                ..
            } => {
                let _ = writeln!(
                    s,
                    "q {w:.2} 0 0 {ih:.2} {x:.2} {:.2} cm /Im{index} Do Q",
                    h - y - ih
                );
            }
            PageOp::Art {
                piece,
                x,
                y,
                sx,
                sy,
            } => {
                if let Some(n) = cx.art.iter().position(|q| std::ptr::eq(*q, *piece)) {
                    let _ = writeln!(
                        s,
                        "q {sx:.4} 0 0 {sy:.4} {x:.2} {:.2} cm /Art{n} Do Q",
                        h - y
                    );
                }
            }
            PageOp::Field(field) => {
                s.push_str(&pdf_form::page_ops(field, cx.fonts, h, cx.mode));
            }
            PageOp::Save => s.push_str("q\n"),
            PageOp::Restore => s.push_str("Q\n"),
            PageOp::Clip { path, even_odd } => {
                path_ops(&mut s, path, h);
                s.push_str(if *even_odd { "W* n\n" } else { "W n\n" });
            }
            PageOp::Opacity(a) => {
                if let Some(gs) = gs_name(*a) {
                    let _ = writeln!(s, "/{gs} gs");
                }
            }
            PageOp::BeginTag { tag, mcid } => match mcid {
                Some(id) => {
                    let _ = writeln!(s, "/{tag} << /MCID {id} >> BDC");
                }
                None => {
                    let _ = writeln!(s, "/{tag} BMC");
                }
            },
            PageOp::Group(_) | PageOp::GroupEnd => {}
            PageOp::Leaf { .. } => s.push_str("/Span BMC\n"),
            PageOp::BeginArtifact => s.push_str("/Artifact << /Type /Layout >> BDC\n"),
            PageOp::EndMarked => s.push_str("EMC\n"),
        }
    }
    s
}

/// Object buffer with an xref offset table.
pub(crate) struct Writer {
    buf: Vec<u8>,
    offsets: Vec<usize>,
    next: usize,
}

impl Writer {
    /// A writer whose header declares PDF 1.4, or 1.6 when `opentype_fonts` (an embedded
    /// `FontFile3 /OpenType` needs 1.6).
    fn new(opentype_fonts: bool) -> Writer {
        let header: &[u8] = if opentype_fonts {
            b"%PDF-1.6\n%\xE2\xE3\xCF\xD3\n"
        } else {
            b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n"
        };
        Writer {
            buf: header.to_vec(),
            offsets: Vec::new(),
            next: FIRST_FREE_OBJ,
        }
    }

    /// Reserve `n` consecutive object ids; returns the first.
    pub fn alloc(&mut self, n: usize) -> usize {
        let id = self.next;
        self.next += n;
        id
    }

    pub fn object(&mut self, id: usize, body: &[u8]) {
        if self.offsets.len() <= id {
            self.offsets.resize(id + 1, 0);
        }
        self.offsets[id] = self.buf.len();
        self.buf
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.buf.extend_from_slice(body);
        self.buf.extend_from_slice(b"\nendobj\n");
    }

    pub fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        let mut body = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.object(id, &body);
    }

    pub fn flate_stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        let packed = compress_to_vec_zlib(data, DEFLATE_LEVEL);
        self.stream(id, &format!("{dict} /Filter /FlateDecode"), &packed);
    }

    fn finish(mut self) -> Vec<u8> {
        let xref_at = self.buf.len();
        let count = self.offsets.len().max(self.next);
        self.offsets.resize(count, 0);
        let mut xref = format!("xref\n0 {count}\n0000000000 65535 f \n");
        for off in &self.offsets[1..] {
            if *off == 0 {
                // A reserved id that was never written: a free entry keeps the table valid.
                xref.push_str("0000000000 65535 f \n");
            } else {
                let _ = writeln!(xref, "{off:010} 00000 n ");
            }
        }
        // File identifier (PDF/A-2 6.1.3): a hash of the body, so the output stays deterministic.
        let id = file_id(&self.buf);
        let _ = write!(
            xref,
            "trailer\n<< /Size {count} /Root {CATALOG_OBJ} 0 R /Info {INFO_OBJ} 0 R /ID [<{id}> <{id}>] >>\n\
             startxref\n{xref_at}\n%%EOF\n"
        );
        self.buf.extend_from_slice(xref.as_bytes());
        self.buf
    }
}

/// Object access for a [`StructureWriter`].
pub(crate) struct ObjectSink<'a> {
    w: &'a mut Writer,
}

impl ObjectSink<'_> {
    pub fn alloc(&mut self) -> usize {
        self.w.alloc(1)
    }

    pub fn object(&mut self, id: usize, body: &str) {
        self.w.object(id, body.as_bytes());
    }

    #[allow(dead_code)] // part of the StructureWriter API
    pub fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        self.w.flate_stream(id, dict, data);
    }
}

fn image_objects(w: &mut Writer, id: usize, smask: Option<usize>, img: &PdfImage) {
    let cs = if img.components == 1 {
        "DeviceGray"
    } else {
        "DeviceRGB"
    };
    let mut dict = format!(
        "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /{cs} /BitsPerComponent 8",
        img.width, img.height
    );
    if let Some(m) = smask {
        let _ = write!(dict, " /SMask {m} 0 R");
    }
    match img.samples {
        Samples::Jpeg => w.stream(id, &format!("{dict} /Filter /DCTDecode"), &img.data),
        Samples::Raw => w.flate_stream(id, &dict, &img.data),
    }
    if let (Some(m), Some(alpha)) = (smask, &img.alpha) {
        let dict = format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8",
            img.width, img.height
        );
        w.flate_stream(m, &dict, alpha);
    }
}

fn font_objects(w: &mut Writer, first: usize, font: &EmbeddedFont) {
    let p = &font.parts;
    let (type0, cid, desc, file, uni) = (first, first + 1, first + 2, first + 3, first + 4);
    w.object(
        type0,
        format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{} /Encoding /Identity-H \
             /DescendantFonts [ {cid} 0 R ] /ToUnicode {uni} 0 R >>",
            p.base_font
        )
        .as_bytes(),
    );
    // CFF subsets are CID-keyed with CID = glyph id (subsetter guarantees it), so neither kind
    // needs more than the identity mapping.
    let (subtype, gid_map, file_key) = if p.cff {
        ("CIDFontType0", "", "FontFile3")
    } else {
        ("CIDFontType2", "/CIDToGIDMap /Identity ", "FontFile2")
    };
    w.object(
        cid,
        format!(
            "<< /Type /Font /Subtype /{subtype} /BaseFont /{} \
             /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
             /FontDescriptor {desc} 0 R {gid_map}/DW 1000 /W {} >>",
            p.base_font, p.widths
        )
        .as_bytes(),
    );
    let [x0, y0, x1, y1] = p.bbox;
    w.object(
        desc,
        format!(
            "<< /Type /FontDescriptor /FontName /{} /Flags 32 /FontBBox [ {x0} {y0} {x1} {y1} ] \
             /ItalicAngle {:.1} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /{file_key} {file} 0 R >>",
            p.base_font, p.italic_angle, p.ascent, p.descent, p.cap_height
        )
        .as_bytes(),
    );
    if p.cff {
        w.flate_stream(file, "/Subtype /OpenType", &p.subset);
    } else {
        w.flate_stream(file, &format!("/Length1 {}", p.subset.len()), &p.subset);
    }
    w.stream(uni, "", p.to_unicode.as_bytes());
}

/// Every glyph the document draws (page text and field values).
fn font_usage(doc: &PdfDocument<'_>) -> Usage {
    let mut usage = Usage::default();
    for op in doc.pages.iter().flat_map(|p| &p.ops) {
        match op {
            PageOp::Text { style, text, .. } => usage.record_text(*style, text),
            PageOp::Field(field) => pdf_form::record_usage(field, &mut usage),
            _ => {}
        }
    }
    usage
}

/// Opacities used anywhere in the document (one ExtGState each).
fn alphas(doc: &PdfDocument<'_>) -> BTreeSet<u16> {
    doc.pages
        .iter()
        .flat_map(|p| &p.ops)
        .flat_map(|op| match op {
            PageOp::Opacity(a) => vec![*a],
            PageOp::Path { paint, .. } => vec![paint.opacity],
            PageOp::Art { piece, .. } => piece.paths.iter().map(|p| p.alpha).collect(),
            _ => Vec::new(),
        })
        .map(alpha_key)
        .filter(|k| *k < 1000)
        .collect()
}

/// Serialise `doc`.
pub(crate) fn write_document(doc: &PdfDocument<'_>) -> Vec<u8> {
    let fonts = EmbeddedFonts::embed(font_usage(doc));
    let mut w = Writer::new(fonts.has_cff());
    let font_res: String = Face::ALL
        .iter()
        .filter_map(|face| {
            let font = fonts.get(*face)?;
            let id = w.alloc(5);
            font_objects(&mut w, id, font);
            Some(format!("/{} {id} 0 R ", face.resource()))
        })
        .collect();
    let xobjs: String = doc
        .images
        .iter()
        .enumerate()
        .map(|(i, img)| {
            let id = w.alloc(1);
            let mask = img.alpha.as_ref().map(|_| w.alloc(1));
            image_objects(&mut w, id, mask, img);
            format!("/Im{i} {id} 0 R ")
        })
        .collect();
    let gstates: String = alphas(doc)
        .into_iter()
        .map(|k| {
            let id = w.alloc(1);
            let a = f32::from(k) / ALPHA_STEPS;
            w.object(
                id,
                format!("<< /Type /ExtGState /ca {a:.3} /CA {a:.3} >>").as_bytes(),
            );
            format!("/GA{k} {id} 0 R ")
        })
        .collect();
    let mut art: Vec<&'static ArtPiece> = Vec::new();
    for page in &doc.pages {
        collect_art(&page.ops, &mut art);
    }
    let art_xobjs: String = art
        .iter()
        .enumerate()
        .map(|(n, piece)| {
            let id = w.alloc(1);
            let (bw, bh) = (piece.width, piece.height);
            w.flate_stream(
                id,
                &format!(
                    "/Type /XObject /Subtype /Form /FormType 1 /BBox [ {:.0} {:.0} {:.0} {:.0} ] \
                     /Resources << /ExtGState << {gstates}>> >>",
                    -2.0 * bw,
                    -2.0 * bh,
                    2.0 * bw,
                    2.0 * bh
                ),
                art_form_content(piece).as_bytes(),
            );
            format!("/Art{n} {id} 0 R ")
        })
        .collect();
    let resources = format!(
        "<< /Font << {font_res}>> /XObject << {xobjs}{art_xobjs}>> /ExtGState << {gstates}>> >>"
    );
    let page_ids: Vec<usize> = doc.pages.iter().map(|_| w.alloc(2)).collect();
    let mut form =
        (doc.mode == FormMode::Fillable).then(|| pdf_form::FormWriter::plan(&doc.pages, &mut w));
    let cx = PageCtx {
        art: &art,
        page_h: doc.page_h,
        fonts: &fonts,
        mode: doc.mode,
    };
    let mut widgets = Vec::with_capacity(doc.pages.len());
    for (page, &page_obj) in doc.pages.iter().zip(&page_ids) {
        let annots = match form.as_mut() {
            Some(f) => f.write_page_widgets(&mut w, page, page_obj, doc.page_h, &fonts, &font_res),
            None => Vec::new(),
        };
        let mut body = format!(
            "<< /Type /Page /Parent {PAGES_OBJ} 0 R /MediaBox [0 0 {:.0} {:.0}] \
             /Resources {resources} /Contents {} 0 R",
            doc.page_w,
            doc.page_h,
            page_obj + 1
        );
        if !annots.is_empty() {
            let refs: String = annots.iter().map(|id| format!("{id} 0 R ")).collect();
            let _ = write!(body, " /Annots [ {refs}] /Tabs /S");
        }
        if let Some(sp) = page.struct_parents {
            let _ = write!(body, " /StructParents {sp}");
        }
        body.push_str(" >>");
        w.object(page_obj, body.as_bytes());
        w.flate_stream(page_obj + 1, "", page_content(&page.ops, &cx).as_bytes());
        widgets.push(annots);
    }
    let acro = form.and_then(|f| f.finish(&mut w));
    let extra = doc.structure.map_or_else(String::new, |s| {
        let refs = DocRefs {
            pages: page_ids.clone(),
            widgets,
        };
        s.write(&mut ObjectSink { w: &mut w }, &refs)
    });
    let kids: String = page_ids.iter().map(|id| format!("{id} 0 R ")).collect();
    w.object(
        PAGES_OBJ,
        format!(
            "<< /Type /Pages /Count {} /Kids [ {kids}] >>",
            page_ids.len()
        )
        .as_bytes(),
    );
    // PDF/UA-1 7.1-10: viewers show the document title, not the file name.
    let mut catalog = format!(
        "<< /Type /Catalog /Pages {PAGES_OBJ} 0 R /ViewerPreferences << /DisplayDocTitle true >>"
    );
    if let Some(id) = acro {
        let _ = write!(catalog, " /AcroForm {id} 0 R");
    }
    if let Some(lang) = &doc.lang {
        let _ = write!(catalog, " /Lang {}", pdf_string(lang));
    }
    if !extra.is_empty() {
        let _ = write!(catalog, " {extra}");
    }
    // PDF/UA-1 7.1-8 and PDF/A-2 6.6.2.1 (XMP), PDF/A-2 6.2.4.3 (output intent).
    let meta = crate::pdf_meta::write_metadata(&mut w, &doc.title);
    let _ = write!(catalog, " {meta} >>");
    w.object(CATALOG_OBJ, catalog.as_bytes());
    w.object(
        INFO_OBJ,
        format!(
            "<< /Title {} /Producer ({}) >>",
            utf16_hex_string(&doc.title),
            crate::pdf_meta::PRODUCER
        )
        .as_bytes(),
    );
    w.finish()
}
