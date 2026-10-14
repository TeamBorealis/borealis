//! Character maps of PDF fonts: `ToUnicode` CMaps, glyph names of `Differences` arrays, and the
//! normalisation of decoded text (ligatures expanded, hyphen variants unified).

use std::collections::HashMap;

use lopdf::{dictionary, Document, Object};

/// Most codes one `bfrange` may cover (a hostile CMap cannot allocate without bound).
const MAX_RANGE: u32 = 0x1_0000;

/// Bytes of a hex string `<...>` (odd digit counts padded with 0, as the spec says).
fn hex_bytes(s: &str) -> Vec<u8> {
    let digits: Vec<u8> = s
        .bytes()
        .filter_map(|b| (b as char).to_digit(16).map(|d| d as u8))
        .collect();
    digits
        .chunks(2)
        .map(|c| (c[0] << 4) | c.get(1).copied().unwrap_or(0))
        .collect()
}

fn code_of(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .take(4)
        .fold(0u32, |a, b| (a << 8) | u32::from(*b))
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|c| (u16::from(c[0]) << 8) | u16::from(c.get(1).copied().unwrap_or(0)))
        .collect();
    String::from_utf16_lossy(&units)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Hex(String),
    Word(String),
    Open,
    Close,
}

fn tokens(src: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = src.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '<' => {
                let mut h = String::new();
                for d in it.by_ref() {
                    if d == '>' {
                        break;
                    }
                    h.push(d);
                }
                out.push(Tok::Hex(h));
            }
            '[' => out.push(Tok::Open),
            ']' => out.push(Tok::Close),
            '%' => {
                for d in it.by_ref() {
                    if d == '\n' || d == '\r' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                let mut w = String::from(c);
                while let Some(&d) = it.peek() {
                    if d.is_whitespace() || "<>[]%".contains(d) {
                        break;
                    }
                    w.push(d);
                    it.next();
                }
                out.push(Tok::Word(w));
            }
        }
    }
    out
}

/// `src` code -> text of a `ToUnicode` CMap (`bfchar` and `bfrange` sections).
pub(crate) fn parse_to_unicode(data: &[u8]) -> HashMap<u32, String> {
    let toks = tokens(&String::from_utf8_lossy(data));
    let mut map = HashMap::new();
    let mut i = 0;
    let mut section = "";
    while i < toks.len() {
        match &toks[i] {
            Tok::Word(w) if w == "beginbfchar" || w == "beginbfrange" => {
                section = if w == "beginbfchar" { "char" } else { "range" };
                i += 1;
            }
            Tok::Word(w) if w.starts_with("end") => {
                section = "";
                i += 1;
            }
            Tok::Hex(src) if section == "char" => {
                if let Some(Tok::Hex(dst)) = toks.get(i + 1) {
                    map.insert(code_of(&hex_bytes(src)), utf16(&hex_bytes(dst)));
                }
                i += 2;
            }
            Tok::Hex(lo) if section == "range" => {
                let Some(Tok::Hex(hi)) = toks.get(i + 1) else {
                    i += 1;
                    continue;
                };
                let (lo, hi) = (code_of(&hex_bytes(lo)), code_of(&hex_bytes(hi)));
                let hi = hi.min(lo.saturating_add(MAX_RANGE));
                match toks.get(i + 2) {
                    Some(Tok::Hex(dst)) => {
                        let base = hex_bytes(dst);
                        for (k, code) in (lo..=hi).enumerate() {
                            let mut b = base.clone();
                            // Increment the last UTF-16 unit.
                            if b.len() >= 2 {
                                let n = b.len();
                                let last = (u16::from(b[n - 2]) << 8 | u16::from(b[n - 1]))
                                    .wrapping_add(k as u16);
                                b[n - 2] = (last >> 8) as u8;
                                b[n - 1] = (last & 0xFF) as u8;
                            }
                            map.insert(code, utf16(&b));
                        }
                        i += 3;
                    }
                    Some(Tok::Open) => {
                        let mut j = i + 3;
                        let mut code = lo;
                        while let Some(Tok::Hex(dst)) = toks.get(j) {
                            if code <= hi {
                                map.insert(code, utf16(&hex_bytes(dst)));
                            }
                            code = code.saturating_add(1);
                            j += 1;
                        }
                        i = j + 1;
                    }
                    _ => i += 2,
                }
            }
            _ => i += 1,
        }
    }
    map
}

/// Text of a glyph name with lopdf's glyph list (`None` when the list does not know it).
fn listed_glyph(doc: &Document, name: &[u8]) -> Option<String> {
    let font = dictionary! {
        "Type" => "Font",
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "Differences" => vec![Object::Integer(0), Object::Name(name.to_vec())],
        },
    };
    let enc = font.get_font_encoding(doc).ok()?;
    let mut out = String::new();
    enc.write_to_string(&[0], &mut out).ok()?;
    (!out.is_empty()).then_some(out)
}

fn hex_glyph(hex: &str) -> Option<String> {
    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp).map(String::from)
}

/// Text of a glyph name: the glyph list, then `uniXXXX`/`uXXXX`, ligature names (`f_i`), and
/// suffixed variants (`one.oldstyle`, `a.sc`). Unknown names (`g12`, `cid0042`) are `None`.
pub(crate) fn glyph_text(doc: &Document, name: &[u8], cache: &mut GlyphCache) -> Option<String> {
    if let Some(hit) = cache.get(name) {
        return hit.clone();
    }
    let s = String::from_utf8_lossy(name).into_owned();
    let found = glyph_text_uncached(doc, &s);
    cache.insert(name.to_vec(), found.clone());
    found
}

/// Glyph names already resolved.
pub(crate) type GlyphCache = HashMap<Vec<u8>, Option<String>>;

fn glyph_text_uncached(doc: &Document, s: &str) -> Option<String> {
    let base = s.split('.').next().unwrap_or(s);
    if base.is_empty() {
        return None;
    }
    if let Some(t) = listed_glyph(doc, base.as_bytes()) {
        return Some(t);
    }
    if let Some(hex) = base.strip_prefix("uni") {
        if hex.len() >= 4 && hex.len() % 4 == 0 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return hex
                .as_bytes()
                .chunks(4)
                .map(|c| hex_glyph(std::str::from_utf8(c).ok()?))
                .collect();
        }
    }
    if let Some(hex) = base.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return hex_glyph(hex);
        }
    }
    if base.contains('_') {
        return base
            .split('_')
            .map(|p| glyph_text_uncached(doc, p))
            .collect();
    }
    match base {
        "ff" | "fi" | "fl" | "ffi" | "ffl" | "st" | "ft" => Some(base.to_string()),
        _ => None,
    }
}

/// Decoded text made plain: typographic ligatures spelled out, hyphen variants and the soft
/// hyphen as `-`, no-break spaces as spaces, control characters dropped.
pub(crate) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
            '\u{00AD}' | '\u{2010}' | '\u{2011}' => out.push('-'),
            '\u{00A0}' | '\u{2002}'..='\u{200A}' => out.push(' '),
            '\u{FFFD}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_unicode_chars_and_ranges() {
        let cmap = b"/CIDInit /ProcSet findresource begin\n2 beginbfchar\n<03> <0041>\n<04> <FB01>\nendbfchar\n\
1 beginbfrange\n<10> <12> <0061>\nendbfrange\n1 beginbfrange\n<20> <21> [<0031> <00660066>]\nendbfrange\nend";
        let m = parse_to_unicode(cmap);
        assert_eq!(m.get(&3).map(String::as_str), Some("A"));
        assert_eq!(m.get(&0x12).map(String::as_str), Some("c"));
        assert_eq!(m.get(&0x21).map(String::as_str), Some("ff"));
        assert_eq!(plain(&m[&4]), "fi");
    }

    #[test]
    fn glyph_names_resolve() {
        let doc = Document::with_version("1.5");
        let mut c = GlyphCache::new();
        let mut g = |n: &str| glyph_text(&doc, n.as_bytes(), &mut c);
        assert_eq!(g("one.oldstyle").as_deref(), Some("1"));
        assert_eq!(g("uni0041").as_deref(), Some("A"));
        assert_eq!(plain(&g("f_l").unwrap()), "fl");
        assert_eq!(g("g37"), None);
        assert_eq!(plain(&g("fi").unwrap()), "fi");
    }
}
