//! Font facts the PDF reader needs: the text of each code (`ToUnicode` first, then the encoding
//! with its `Differences`), whether the face is bold, and glyph widths (so word gaps can be told
//! from letter gaps).

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::pdf_cmap::{glyph_text, parse_to_unicode, plain, GlyphCache};

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
pub(crate) struct FontInfo {
    /// The face is bold (by its name).
    pub bold: bool,
    widths: Widths,
    /// Codes are two bytes (Type0 fonts).
    two_byte: bool,
    /// The font's `ToUnicode` map (takes precedence over the encoding, as the spec says).
    to_unicode: HashMap<u32, String>,
    /// One-byte code -> text from the encoding and its `Differences` (simple fonts).
    base: Vec<Option<String>>,
    /// Glyph widths -> text space (a Type 3 font's `FontMatrix`; 1/1000 otherwise).
    glyph_scale: f32,
    /// Factor from the selected font size to the printed size (Type 3 fonts).
    pub size_scale: f32,
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

/// Fonts parsed once per document (by object id), and glyph names already resolved.
#[derive(Default)]
pub(crate) struct FontCache {
    fonts: HashMap<ObjectId, std::rc::Rc<FontInfo>>,
    glyphs: GlyphCache,
}

fn known_base(name: &[u8]) -> bool {
    matches!(
        name,
        b"StandardEncoding" | b"WinAnsiEncoding" | b"MacRomanEncoding" | b"MacExpertEncoding"
    )
}

/// One-byte code -> text of a simple font: its base encoding, then its `Differences`.
fn base_table(doc: &Document, font: &Dictionary, glyphs: &mut GlyphCache) -> Vec<Option<String>> {
    let truetype = font.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"TrueType");
    let default: &[u8] = if truetype {
        b"WinAnsiEncoding"
    } else {
        b"StandardEncoding"
    };
    let (named, diffs) = match font.get(b"Encoding").map(|o| deref(doc, o)) {
        Ok(Object::Name(n)) => (Some(n.as_slice()), None),
        Ok(Object::Dictionary(d)) => (
            d.get(b"BaseEncoding").and_then(Object::as_name).ok(),
            d.get(b"Differences")
                .ok()
                .and_then(|o| deref(doc, o).as_array().ok()),
        ),
        _ => (None, None),
    };
    let name = named.filter(|n| known_base(n)).unwrap_or(default).to_vec();
    let probe = lopdf::dictionary! {"Type" => "Font", "Encoding" => Object::Name(name)};
    let mut table: Vec<Option<String>> = vec![None; 256];
    if let Ok(enc) = probe.get_font_encoding(doc) {
        for (b, slot) in table.iter_mut().enumerate() {
            let mut t = String::new();
            if enc.write_to_string(&[b as u8], &mut t).is_ok() && !t.is_empty() {
                *slot = Some(t);
            }
        }
    }
    let mut code = 0usize;
    for o in diffs.into_iter().flatten() {
        match deref(doc, o) {
            Object::Integer(c) => code = usize::try_from(*c).unwrap_or(usize::MAX),
            Object::Name(n) => {
                if let Some(slot) = table.get_mut(code) {
                    *slot = glyph_text(doc, n, glyphs);
                }
                code = code.saturating_add(1);
            }
            _ => {}
        }
    }
    table
}

fn to_unicode(doc: &Document, font: &Dictionary) -> HashMap<u32, String> {
    font.get(b"ToUnicode")
        .ok()
        .and_then(|o| deref(doc, o).as_stream().ok())
        .and_then(|s| s.get_plain_content_with_limit(MAX_FONT_STREAM).ok())
        .map(|data| parse_to_unicode(&data))
        .unwrap_or_default()
}

fn font_info(doc: &Document, font: &Dictionary, glyphs: &mut GlyphCache) -> FontInfo {
    let subtype = font.get(b"Subtype").and_then(Object::as_name).ok();
    let two_byte = subtype == Some(b"Type0");
    let widths = if two_byte {
        cid_widths(doc, font)
    } else {
        simple_widths(doc, font)
    };
    let matrix: Vec<f32> = font
        .get(b"FontMatrix")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
        .map(|a| a.iter().map(|o| num(deref(doc, o))).collect())
        .unwrap_or_default();
    let (glyph_scale, size_scale) = if subtype == Some(b"Type3") && matrix.len() == 6 {
        let s = matrix[3].abs().max(matrix[0].abs());
        (matrix[0].abs(), if s > 0.0 { s * 1000.0 } else { 1.0 })
    } else {
        (0.001, 1.0)
    };
    let mut unicode = to_unicode(doc, font);
    let base = if two_byte {
        Vec::new()
    } else {
        base_table(doc, font, glyphs)
    };
    if !two_byte && glyph_scale == 0.001 && reencoded(&unicode, &base) {
        unicode = repair_identity_fallbacks(unicode, |c| width_of(&widths, glyph_scale, c));
    }
    FontInfo {
        bold: is_bold_font(font),
        widths,
        two_byte,
        to_unicode: unicode,
        base,
        glyph_scale,
        size_scale,
    }
}

fn width_of(widths: &Widths, glyph_scale: f32, code: u32) -> f32 {
    match widths {
        Widths::Simple { first, w, missing } => code
            .checked_sub(*first)
            .and_then(|i| w.get(i as usize))
            .copied()
            .filter(|x| *x > 0.0)
            .unwrap_or(*missing),
        Widths::Cid { map, default } => map.get(&code).copied().unwrap_or(*default),
        Widths::Unknown => DEFAULT_WIDTH / (glyph_scale * 1000.0).max(f32::EPSILON),
    }
}

/// The codes do not follow the font's encoding: most `ToUnicode` texts differ from it (a
/// subset font numbered in its own order). Only then is a code's own byte as its text a sign
/// of a glyph the producer could not name.
fn reencoded(map: &HashMap<u32, String>, base: &[Option<String>]) -> bool {
    let agree = map
        .iter()
        .filter(|(c, t)| base.get(**c as usize).and_then(Option::as_ref) == Some(*t))
        .count();
    !map.is_empty() && 2 * agree < map.len()
}

/// A glyph narrower than this share of the glyph sharing its text is not that character.
const FALLBACK_NARROW: f32 = 0.75;
/// Widest glyph (thousandths of the font size) read as a word space, then as a hyphen.
const SPACE_WIDTH_MAX: f32 = 280.0;
const HYPHEN_WIDTH_MAX: f32 = 420.0;

/// Some producers write a code's own byte as its `ToUnicode` text when they cannot name the
/// glyph (the space and the hyphen, whose characters have several code points). Such a code
/// collides with the code really mapped to that character and is much narrower than it; by its
/// width it is read as a word space or a hyphen ("Ember-Lit" printed with `O` for the hyphen
/// decodes as "EmberOLit" otherwise). Other codes are kept.
fn repair_identity_fallbacks(
    map: HashMap<u32, String>,
    width: impl Fn(u32) -> f32,
) -> HashMap<u32, String> {
    let identity = |code: u32, t: &str| {
        let mut chars = t.chars();
        chars.next().map(u32::from) == Some(code) && chars.next().is_none()
    };
    let repaired = |code: u32, t: &str| -> Option<String> {
        if !identity(code, t) {
            return None;
        }
        let w = width(code);
        let wider = map
            .iter()
            .any(|(c, u)| *c != code && u == t && w < FALLBACK_NARROW * width(*c));
        if !wider {
            return None;
        }
        if w <= SPACE_WIDTH_MAX {
            Some(" ".into())
        } else if w <= HYPHEN_WIDTH_MAX {
            Some("-".into())
        } else {
            None
        }
    };
    map.iter()
        .map(|(c, t)| (*c, repaired(*c, t).unwrap_or_else(|| t.clone())))
        .collect()
}

impl FontCache {
    /// The fonts of a resource dictionary by resource name.
    pub(crate) fn fonts_of(
        &mut self,
        doc: &Document,
        resources: &[&Dictionary],
    ) -> HashMap<Vec<u8>, std::rc::Rc<FontInfo>> {
        let mut out = HashMap::new();
        for res in resources {
            let Some(fonts) = res
                .get(b"Font")
                .ok()
                .and_then(|o| deref(doc, o).as_dict().ok())
            else {
                continue;
            };
            for (name, value) in fonts {
                if out.contains_key(name) {
                    continue;
                }
                let Ok(dict) = deref(doc, value).as_dict() else {
                    continue;
                };
                let info = match value.as_reference() {
                    Ok(id) => self
                        .fonts
                        .entry(id)
                        .or_insert_with(|| std::rc::Rc::new(font_info(doc, dict, &mut self.glyphs)))
                        .clone(),
                    Err(_) => std::rc::Rc::new(font_info(doc, dict, &mut self.glyphs)),
                };
                out.insert(name.clone(), info);
            }
        }
        out
    }
}

impl FontInfo {
    /// Text of one code: the `ToUnicode` entry when it is usable, else the encoding.
    fn decode_code(&self, code: u32, out: &mut String) {
        if let Some(t) = self.to_unicode.get(&code) {
            let usable = !t.is_empty()
                && t.chars().all(|c| {
                    !c.is_control() && c != '\u{FFFD}' && !('\u{E000}'..='\u{F8FF}').contains(&c)
                });
            if usable {
                out.push_str(t);
                return;
            }
        }
        if let Some(Some(t)) = self.base.get(code as usize) {
            out.push_str(t);
        }
    }

    fn decode(&self, bytes: &[u8], out: &mut String) {
        let step = if self.two_byte { 2 } else { 1 };
        for c in bytes.chunks(step) {
            let code = c.iter().fold(0u32, |a, b| (a << 8) | u32::from(*b));
            self.decode_code(code, out);
        }
    }

    fn width(&self, code: u32) -> f32 {
        width_of(&self.widths, self.glyph_scale, code)
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
                (self.width(code) * self.glyph_scale * sp.size + sp.char_space + word) * sp.scale
            })
            .sum()
    }
}

/// Decode the strings of a show operator; returns the text and its advance in text space.
pub(crate) fn shown(font: Option<&FontInfo>, operands: &[Object], sp: &Spacing) -> (String, f32) {
    let mut text = String::new();
    let mut advance = 0.0;
    for o in operands {
        match o {
            Object::String(bytes, _) => {
                let before = text.chars().count();
                if let Some(f) = font {
                    let mut raw = String::new();
                    f.decode(bytes, &mut raw);
                    text.push_str(&plain(&raw));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_narrow_code_mapped_to_its_own_byte_beside_the_real_letter_is_a_hyphen_or_space() {
        let map: HashMap<u32, String> = [
            (0x4f, "O"),
            (0x58, "O"),
            (0x21, "!"),
            (0x30, "!"),
            (0x41, "A"),
            (0x50, "P"),
        ]
        .into_iter()
        .map(|(c, t)| (c, t.to_string()))
        .collect();
        let widths = |c: u32| match c {
            0x4f => 330.0,
            0x21 => 220.0,
            0x30 => 500.0,
            0x50 => 330.0,
            _ => 700.0,
        };
        let fixed = repair_identity_fallbacks(map, widths);
        assert_eq!(fixed[&0x4f], "-");
        assert_eq!(fixed[&0x58], "O");
        assert_eq!(fixed[&0x21], " ");
        assert_eq!(fixed[&0x41], "A");
        // Narrow but alone with its text: kept.
        assert_eq!(fixed[&0x50], "P");
    }

    #[test]
    fn only_a_font_numbered_in_its_own_order_counts_as_reencoded() {
        let mut base: Vec<Option<String>> = vec![None; 256];
        for c in 0x41u8..=0x5a {
            base[c as usize] = Some((c as char).to_string());
        }
        let own_order: HashMap<u32, String> = (0x41u32..=0x46)
            .map(|c| (c, char::from_u32(c + 6).unwrap().to_string()))
            .collect();
        assert!(reencoded(&own_order, &base));
        let follows: HashMap<u32, String> = (0x41u32..=0x46)
            .map(|c| (c, char::from_u32(c).unwrap().to_string()))
            .collect();
        assert!(!reencoded(&follows, &base));
    }

    /// Offline look at a PDF's fonts: `BOREALIS_PDF_FONT=<file.pdf>:<object id>` prints the
    /// codes whose `ToUnicode` text differs from the encoding, with their widths.
    #[test]
    #[ignore = "needs a local PDF"]
    fn probe_font_codes() {
        let Ok(arg) = std::env::var("BOREALIS_PDF_FONT") else {
            return;
        };
        let (file, id) = arg.rsplit_once(':').unwrap();
        let doc = Document::load_mem(&std::fs::read(file).unwrap()).unwrap();
        let dict = doc.get_dictionary((id.parse().unwrap(), 0)).unwrap();
        let info = font_info(&doc, dict, &mut GlyphCache::default());
        let mut codes: Vec<_> = info.to_unicode.iter().collect();
        codes.sort();
        for (code, t) in codes {
            let base = info.base.get(*code as usize).cloned().flatten();
            println!("{code:#04x} {t:?} base={base:?} w={}", info.width(*code));
        }
    }
}
