//! Whether an image can print text, from its own pixels. Printed text (a stat block, a table, a
//! rules box pasted as a picture) is lines of ink on a flat ground: rows with ink alternate with
//! empty rows, line after line. Art and ornaments have no such ground or no such rows. Only
//! images whose pixels can be read here (8-bit gray, RGB or CMYK, uncompressed or
//! Flate-compressed) are judged; any other image may print text.

use lopdf::{Document, Object, Stream};

/// What an image's pixels say about printed text in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ink {
    /// Lines of text on a flat ground.
    Text,
    /// Readable pixels with no lines of text: art, an ornament, a logo mark.
    NoText,
    /// The pixels could not be read (JPEG, indexed colours, too few pixels): it may hold text.
    Unknown,
}

/// Most decoded bytes per image (a full-page scan at print resolution is far less).
const MAX_DECODED: usize = 48 * 1024 * 1024;
/// Fewest pixels per side before the pixels are judged at all.
const MIN_PIXELS: usize = 32;
/// Most columns sampled per row.
const MAX_COLUMNS: usize = 512;
/// Luminance histogram bins.
const BINS: usize = 16;
/// A pixel this close (luminance) to the ground is ground.
const GROUND_SPREAD: i32 = 24;
/// A pixel this far from the ground is ink.
const INK_CONTRAST: i32 = 64;
/// Fewest share of ground pixels text sits on.
const MIN_GROUND_SHARE: f32 = 0.35;
/// Vertical strips judged separately (a portrait beside a stat block fills every row of the
/// image, not of the strip the text is in).
const STRIPS: usize = 4;
/// Share of a strip's samples that makes a row an ink row.
const INK_ROW_SHARE: f32 = 0.01;
/// Fewest separate lines that make text (a logo word is one or two).
const MIN_TEXT_LINES: usize = 3;
/// Tallest share of the image one line of text takes.
const MAX_LINE_SHARE: f32 = 0.25;
/// Alpha below which a pixel is transparent (ground).
const OPAQUE: u8 = 128;

/// Gray pixels (and alpha, when the image has a soft mask), row by row.
pub(super) struct Pixels {
    pub width: usize,
    pub height: usize,
    pub gray: Vec<u8>,
    pub alpha: Option<Vec<u8>>,
}

fn int(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<usize> {
    let o = dict.get(key).ok()?;
    let o = doc.dereference(o).map(|x| x.1).unwrap_or(o);
    o.as_i64().ok().and_then(|v| usize::try_from(v).ok())
}

/// Colour components of a colour space this module reads (gray 1, RGB 3, CMYK 4).
fn components(doc: &Document, cs: &Object) -> Option<usize> {
    let cs = doc.dereference(cs).map(|x| x.1).unwrap_or(cs);
    match cs {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some(1),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(3),
            b"DeviceCMYK" | b"CMYK" => Some(4),
            _ => None,
        },
        Object::Array(a) => match a.first() {
            Some(Object::Name(n)) if n == b"ICCBased" => {
                let id = a.get(1)?.as_reference().ok()?;
                let s = doc.get_object(id).ok()?.as_stream().ok()?;
                int(doc, &s.dict, b"N").filter(|n| matches!(n, 1 | 3 | 4))
            }
            Some(Object::Name(n)) if matches!(n.as_slice(), b"CalRGB") => Some(3),
            Some(Object::Name(n)) if matches!(n.as_slice(), b"CalGray") => Some(1),
            _ => None,
        },
        _ => None,
    }
}

/// The 8-bit samples of `s` (`n` per pixel), when they can be read.
fn samples(doc: &Document, s: &Stream, n: usize) -> Option<(usize, usize, Vec<u8>)> {
    let w = int(doc, &s.dict, b"Width")?;
    let h = int(doc, &s.dict, b"Height")?;
    if int(doc, &s.dict, b"BitsPerComponent") != Some(8) || w < MIN_PIXELS || h < MIN_PIXELS {
        return None;
    }
    let need = w.checked_mul(h)?.checked_mul(n)?;
    if need > MAX_DECODED {
        return None;
    }
    let data = if s.dict.get(b"Filter").is_err() {
        s.content.clone()
    } else {
        s.decompressed_content_with_limit(MAX_DECODED).ok()?
    };
    (data.len() >= need).then_some((w, h, data))
}

fn luminance(px: &[u8]) -> u8 {
    match px {
        [g] => *g,
        [r, g, b] => {
            ((u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000) as u8
        }
        [c, m, y, k] => {
            let ink = (u32::from(*c) * 3 + u32::from(*m) * 6 + u32::from(*y)) / 10 + u32::from(*k);
            255 - ink.min(255) as u8
        }
        _ => 0,
    }
}

/// The pixels of the image stream `s`, when this module can read them.
pub(super) fn pixels(doc: &Document, s: &Stream) -> Option<Pixels> {
    if s.dict
        .get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let n = components(doc, s.dict.get(b"ColorSpace").ok()?)?;
    let (width, height, data) = samples(doc, s, n)?;
    let gray: Vec<u8> = data[..width * height * n]
        .chunks(n)
        .map(luminance)
        .collect();
    let alpha = s
        .dict
        .get(b"SMask")
        .and_then(Object::as_reference)
        .ok()
        .and_then(|id| doc.get_object(id).ok()?.as_stream().ok())
        .and_then(|m| samples(doc, m, 1))
        .filter(|(w, h, _)| *w == width && *h == height)
        .map(|(_, _, a)| a[..width * height].to_vec());
    Some(Pixels {
        width,
        height,
        gray,
        alpha,
    })
}

/// A sampled pixel: ground, a tone between, or ink.
const GROUND: u8 = 0;
const TONE: u8 = 1;
const INKED: u8 = 2;

/// Separate lines of ink in the columns `cols` of the classified rows: runs of rows that are
/// not ground, holding ink, no taller than a line of text. Only a row of ground parts two
/// lines (the gap between printed lines is paper; art's tones are not).
fn lines_in(rows: &[Vec<u8>], cols: std::ops::Range<usize>, height: usize) -> usize {
    let wanted = ((cols.len() as f32 * INK_ROW_SHARE).ceil() as usize).max(2);
    let tallest = ((height as f32 * MAX_LINE_SHARE) as usize).max(1);
    let mut lines = 0;
    let (mut run, mut inked) = (0usize, false);
    for row in rows {
        let cells = &row[cols.clone()];
        let off_ground = cells.iter().filter(|c| **c != GROUND).count();
        if off_ground < wanted {
            lines += usize::from(inked && run <= tallest);
            (run, inked) = (0, false);
            continue;
        }
        run += 1;
        inked |= cells.iter().filter(|c| **c == INKED).count() >= wanted;
    }
    lines + usize::from(inked && run <= tallest)
}

/// What the pixels `p` say: lines of text on a flat ground, or no text.
pub(super) fn judge(p: &Pixels) -> Ink {
    if p.width < MIN_PIXELS || p.height < MIN_PIXELS || p.gray.len() < p.width * p.height {
        return Ink::Unknown;
    }
    let step = p.width.div_ceil(MAX_COLUMNS).max(1);
    let opaque = |i: usize| p.alpha.as_ref().map_or(true, |a| a[i] >= OPAQUE);
    let mut hist = [0usize; BINS];
    for y in 0..p.height {
        for x in (0..p.width).step_by(step) {
            let i = y * p.width + x;
            if opaque(i) {
                hist[usize::from(p.gray[i]) * BINS / 256] += 1;
            }
        }
    }
    let Some(mode) = (0..BINS).max_by_key(|b| hist[*b]) else {
        return Ink::Unknown;
    };
    let ground = (mode * 256 / BINS + 128 / BINS) as i32;
    let mut ground_count = 0usize;
    let mut total = 0usize;
    let rows: Vec<Vec<u8>> = (0..p.height)
        .map(|y| {
            (0..p.width)
                .step_by(step)
                .map(|x| {
                    let i = y * p.width + x;
                    total += 1;
                    let d = if opaque(i) {
                        (i32::from(p.gray[i]) - ground).abs()
                    } else {
                        0
                    };
                    if d <= GROUND_SPREAD {
                        ground_count += 1;
                        GROUND
                    } else if d >= INK_CONTRAST {
                        INKED
                    } else {
                        TONE
                    }
                })
                .collect()
        })
        .collect();
    if (ground_count as f32) < MIN_GROUND_SHARE * total.max(1) as f32 {
        return Ink::NoText;
    }
    let cols = rows.first().map_or(0, Vec::len);
    let strip = cols.div_ceil(STRIPS).max(1);
    // The whole width, then each strip.
    let strips = (0..cols).step_by(strip).map(|s| s..(s + strip).min(cols));
    if std::iter::once(0..cols)
        .chain(strips)
        .any(|r| lines_in(&rows, r, p.height) >= MIN_TEXT_LINES)
    {
        Ink::Text
    } else {
        Ink::NoText
    }
}

/// What the image stream `s` says about text in it.
pub(super) fn ink_of(doc: &Document, s: &Stream) -> Ink {
    pixels(doc, s).map_or(Ink::Unknown, |p| judge(&p))
}
