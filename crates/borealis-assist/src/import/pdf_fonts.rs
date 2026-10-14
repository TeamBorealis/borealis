//! Font facts the PDF reader needs: the text encoding, whether the face is bold, and glyph
//! widths (so word gaps can be told from letter gaps).

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};

/// Average glyph advance (thousandths of the font size) when a font declares no widths.
const DEFAULT_WIDTH: f32 = 500.0;
/// A TJ adjustment more negative than this (thousandths of the font size) is a word space.
const TJ_SPACE: f32 = -200.0;
/// Decompression cap for font streams (encodings, CMaps).
const MAX_FONT_STREAM: usize = 32 * 1024 * 1024;

/// Glyph widths in thousandths of the font size.
#[derive(Debug, Clone, Default)]
enum Widths {
    /// Simple font: `Widths` from `FirstChar`, `MissingWidth` otherwise.
    Simple {
        first: u32,
        w: Vec<f32>,
        missing: f32,
    },
    /// CID font: `W` ranges, `DW` otherwise.
    Cid {
        map: HashMap<u32, f32>,
        default: f32,
    },
    /// No metrics.
    #[default]
    Unknown,
}

/// One font of a page.
pub(crate) struct FontInfo<'a> {
    encoding: Option<Encoding<'a>>,
    /// The face is bold (by its name).
    pub bold: bool,
    widths: Widths,
    /// Codes are two bytes (Type0 fonts).
    two_byte: bool,
}

/// Text state that changes advances (`Tc`, `Tw`, `Tz`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spacing {
    pub size: f32,
    pub char_space: f32,
    pub word_space: f32,
    pub scale: f32,
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map_or(o, |(_, x)| x)
}

fn num(o: &Object) -> f32 {
    o.as_float().unwrap_or(0.0)
}

fn is_bold_font(font: &Dictionary) -> bool {
    let name = font
        .get(b"BaseFont")
        .and_then(Object::as_name)
        .map(|n| String::from_utf8_lossy(n).to_lowercase())
        .unwrap_or_default();
    ["bold", "black", "heavy", "semibold", "demi"]
        .iter()
        .any(|w| name.contains(w))
}

fn simple_widths(doc: &Document, font: &Dictionary) -> Widths {
    let Ok(Object::Array(w)) = font.get(b"Widths").map(|o| deref(doc, o)) else {
        return Widths::Unknown;
    };
    let first = font
        .get(b"FirstChar")
        .map(|o| num(deref(doc, o)))
        .unwrap_or(0.0) as u32;
    let missing = font
        .get(b"FontDescriptor")
        .ok()
        .and_then(|d| deref(doc, d).as_dict().ok())
        .and_then(|d| d.get(b"MissingWidth").ok())
        .map(|o| num(deref(doc, o)))
        .filter(|w| *w > 0.0)
        .unwrap_or(DEFAULT_WIDTH);
    Widths::Simple {
        first,
        w: w.iter().map(|o| num(deref(doc, o))).collect(),
        missing,
    }
}

fn cid_widths(doc: &Document, font: &Dictionary) -> Widths {
    let Some(desc) = font
        .get(b"DescendantFonts")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
        .and_then(|a| a.first())
        .and_then(|o| deref(doc, o).as_dict().ok())
    else {
        return Widths::Unknown;
    };
    let default = desc
        .get(b"DW")
        .map(|o| num(deref(doc, o)))
        .unwrap_or(1000.0);
    let mut map = HashMap::new();
    if let Ok(Object::Array(w)) = desc.get(b"W").map(|o| deref(doc, o)) {
        let mut i = 0;
        while i + 1 < w.len() {
            let start = num(deref(doc, &w[i])) as u32;
            match deref(doc, &w[i + 1]) {
                Object::Array(list) => {
                    for (k, x) in list.iter().enumerate() {
                        map.insert(start + k as u32, num(deref(doc, x)));
                    }
                    i += 2;
                }
                end if i + 2 < w.len() => {
                    let end = num(end) as u32;
                    let width = num(deref(doc, &w[i + 2]));
                    for c in start..=end.min(start + 0xFFFF) {
                        map.insert(c, width);
                    }
                    i += 3;
                }
                _ => break,
            }
        }
    }
    Widths::Cid { map, default }
}

/// The fonts of a page by resource name.
pub(crate) fn page_fonts(doc: &Document, page: ObjectId) -> HashMap<Vec<u8>, FontInfo<'_>> {
    let mut out = HashMap::new();
    for (name, font) in doc.get_page_fonts(page).unwrap_or_default() {
        let two_byte = font.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type0");
        let widths = if two_byte {
            cid_widths(doc, font)
        } else {
            simple_widths(doc, font)
        };
        out.insert(
            name,
            FontInfo {
                encoding: font.get_font_encoding_with_limit(doc, MAX_FONT_STREAM).ok(),
                bold: is_bold_font(font),
                widths,
                two_byte,
            },
        );
    }
    out
}

impl FontInfo<'_> {
    fn width(&self, code: u32) -> f32 {
        match &self.widths {
            Widths::Simple { first, w, missing } => code
                .checked_sub(*first)
                .and_then(|i| w.get(i as usize))
                .copied()
                .filter(|x| *x > 0.0)
                .unwrap_or(*missing),
            Widths::Cid { map, default } => map.get(&code).copied().unwrap_or(*default),
            Widths::Unknown => DEFAULT_WIDTH,
        }
    }

    /// Horizontal advance of `bytes` in unscaled text space.
    fn advance(&self, bytes: &[u8], sp: &Spacing) -> f32 {
        let step = if self.two_byte { 2 } else { 1 };
        bytes
            .chunks(step)
            .map(|c| {
                let code = c.iter().fold(0u32, |a, b| (a << 8) | u32::from(*b));
                let word = if !self.two_byte && code == 32 {
                    sp.word_space
                } else {
                    0.0
                };
                (self.width(code) / 1000.0 * sp.size + sp.char_space + word) * sp.scale
            })
            .sum()
    }
}

/// Decode the strings of a show operator; returns the text and its advance in text space.
pub(crate) fn shown(
    font: Option<&FontInfo<'_>>,
    operands: &[Object],
    sp: &Spacing,
) -> (String, f32) {
    let mut text = String::new();
    let mut advance = 0.0;
    for o in operands {
        match o {
            Object::String(bytes, _) => {
                let before = text.chars().count();
                if let Some(e) = font.and_then(|f| f.encoding.as_ref()) {
                    let _ = e.write_to_string(bytes, &mut text);
                }
                advance += match font {
                    Some(f) => f.advance(bytes, sp),
                    None => {
                        (text.chars().count() - before) as f32 * DEFAULT_WIDTH / 1000.0
                            * sp.size
                            * sp.scale
                    }
                };
            }
            Object::Array(items) => {
                let (t, a) = shown(font, items, sp);
                text.push_str(&t);
                advance += a;
            }
            Object::Integer(_) | Object::Real(_) => {
                let adj = num(o);
                advance -= adj / 1000.0 * sp.size * sp.scale;
                if adj < TJ_SPACE && !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            _ => {}
        }
    }
    (text, advance)
}
