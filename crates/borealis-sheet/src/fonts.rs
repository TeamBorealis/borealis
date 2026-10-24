//! Embedded OFL fonts (Noto Sans, Noto Serif, Noto Sans Hebrew, Noto Sans Arabic) plus system CJK
//! faces ([`crate::os_fonts`]): glyph lookup with per-style fallback, and advance metrics used by
//! both layout and the PDF writer.

use crate::os_fonts::{self, Slot};
use std::sync::OnceLock;
use ttf_parser::Face as Parsed;

/// A concrete font file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Face {
    Sans,
    SansBold,
    SansItalic,
    SerifBold,
    Hebrew,
    HebrewBold,
    // Appended so the resource names of the older faces (F1..F6) stay stable.
    Serif,
    SerifItalic,
    Arabic,
    ArabicBold,
    /// System font for Han/kana text (not embedded in the binary; see [`crate::os_fonts`]).
    Cjk,
    /// System font for Hangul text.
    Hangul,
    /// Bold companion of the Han system font, when the machine has one.
    CjkBold,
    /// Bold companion of the Hangul system font, when the machine has one.
    HangulBold,
}

/// A text style: what layout and form fields ask for.
#[allow(dead_code)]
// the regular serif styles are only exercised by tests so far Each style resolves to a chain of faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Style {
    Sans,
    SansBold,
    SansItalic,
    Serif,
    SerifBold,
    SerifItalic,
}

impl Style {
    /// The first face of the chain (its cap height places single-line field values).
    pub fn primary(self) -> Face {
        chain(self)[0]
    }
}

impl Face {
    pub const ALL: [Face; 14] = [
        Face::Sans,
        Face::SansBold,
        Face::SansItalic,
        Face::SerifBold,
        Face::Hebrew,
        Face::HebrewBold,
        Face::Serif,
        Face::SerifItalic,
        Face::Arabic,
        Face::ArabicBold,
        Face::Cjk,
        Face::Hangul,
        Face::CjkBold,
        Face::HangulBold,
    ];

    /// The faces compiled into the binary.
    pub const EMBEDDED: [Face; 10] = [
        Face::Sans,
        Face::SansBold,
        Face::SansItalic,
        Face::SerifBold,
        Face::Hebrew,
        Face::HebrewBold,
        Face::Serif,
        Face::SerifItalic,
        Face::Arabic,
        Face::ArabicBold,
    ];

    /// The system-font slot of a face loaded at render time, and whether it is the bold one.
    fn os_slot(self) -> Option<(Slot, bool)> {
        match self {
            Face::Cjk => Some((Slot::Han, false)),
            Face::Hangul => Some((Slot::Hangul, false)),
            Face::CjkBold => Some((Slot::Han, true)),
            Face::HangulBold => Some((Slot::Hangul, true)),
            _ => None,
        }
    }

    /// The loaded system font behind `self`, if it is a system face and the machine has one.
    fn os(self) -> Option<&'static os_fonts::Loaded> {
        self.os_slot()
            .and_then(|(slot, bold)| os_fonts::loaded(slot, bold))
    }

    /// Whether the face is usable: always for embedded faces, for system faces when a font was
    /// found.
    pub fn available(self) -> bool {
        self.os_slot().is_none() || self.os().is_some()
    }

    /// Whether text in this face goes through the OpenType shaper.
    pub fn shaped(self) -> bool {
        matches!(self, Face::Arabic | Face::ArabicBold)
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Page resource name (`/F1`...).
    pub fn resource(self) -> String {
        format!("F{}", self.index() + 1)
    }

    /// PostScript name.
    pub fn ps_name(self) -> &'static str {
        if let Some(os) = self.os() {
            return &os.font.ps_name;
        }
        match self {
            Face::Sans => "NotoSans-Regular",
            Face::SansBold => "NotoSans-Bold",
            Face::SansItalic => "NotoSans-Italic",
            Face::SerifBold => "NotoSerif-Bold",
            Face::Hebrew => "NotoSansHebrew-Regular",
            Face::HebrewBold => "NotoSansHebrew-Bold",
            Face::Serif => "NotoSerif-Regular",
            Face::SerifItalic => "NotoSerif-Italic",
            Face::Arabic => "NotoSansArabic-Regular",
            Face::ArabicBold => "NotoSansArabic-Bold",
            Face::Cjk | Face::Hangul | Face::CjkBold | Face::HangulBold => "SystemCJK",
        }
    }

    /// Font file bytes (empty for a system face the machine lacks).
    pub fn data(self) -> &'static [u8] {
        match self {
            Face::Sans => include_bytes!("../fonts/NotoSans-Regular.ttf"),
            Face::SansBold => include_bytes!("../fonts/NotoSans-Bold.ttf"),
            Face::SansItalic => include_bytes!("../fonts/NotoSans-Italic.ttf"),
            Face::SerifBold => include_bytes!("../fonts/NotoSerif-Bold.ttf"),
            Face::Hebrew => include_bytes!("../fonts/NotoSansHebrew-Regular.ttf"),
            Face::HebrewBold => include_bytes!("../fonts/NotoSansHebrew-Bold.ttf"),
            Face::Serif => include_bytes!("../fonts/NotoSerif-Regular.ttf"),
            Face::SerifItalic => include_bytes!("../fonts/NotoSerif-Italic.ttf"),
            Face::Arabic => include_bytes!("../fonts/NotoSansArabic-Regular.ttf"),
            Face::ArabicBold => include_bytes!("../fonts/NotoSansArabic-Bold.ttf"),
            Face::Cjk | Face::Hangul | Face::CjkBold | Face::HangulBold => {
                self.os().map_or(&[], |o| o.font.data)
            }
        }
    }

    /// Face index inside the font file (non-zero only for system collections).
    pub fn collection_index(self) -> u32 {
        self.os().map_or(0, |o| o.font.index)
    }

    /// The parsed face. A system face the machine lacks never resolves a glyph, so it is never
    /// asked for; it answers with Noto Sans rather than panicking.
    pub fn parsed(self) -> &'static Parsed<'static> {
        static FACES: OnceLock<Vec<Parsed<'static>>> = OnceLock::new();
        if self.os_slot().is_some() {
            return self.os().map_or_else(|| Face::Sans.parsed(), |o| &o.parsed);
        }
        let faces = FACES.get_or_init(|| {
            Face::EMBEDDED
                .iter()
                // The files are compiled in and covered by a unit test, so a parse failure is a
                // build-time invariant violation, not a runtime condition.
                .map(|f| Parsed::parse(f.data(), 0).expect("embedded font parses"))
                .collect()
        });
        &faces[self.index()]
    }

    /// Design units per em.
    pub fn units_per_em(self) -> f32 {
        f32::from(self.parsed().units_per_em())
    }
}

/// Faces tried in order for a style: the Latin/Cyrillic/Greek face, its sans counterpart (serif
/// styles only), then Hebrew and Arabic. System CJK faces come after the chain (see [`glyph`]).
fn chain(style: Style) -> &'static [Face] {
    match style {
        Style::Sans => &[Face::Sans, Face::Hebrew, Face::Arabic],
        Style::SansBold => &[Face::SansBold, Face::HebrewBold, Face::ArabicBold],
        Style::SansItalic => &[Face::SansItalic, Face::Hebrew, Face::Arabic],
        Style::Serif => &[Face::Serif, Face::Sans, Face::Hebrew, Face::Arabic],
        Style::SerifBold => &[
            Face::SerifBold,
            Face::SansBold,
            Face::HebrewBold,
            Face::ArabicBold,
        ],
        Style::SerifItalic => &[
            Face::SerifItalic,
            Face::SansItalic,
            Face::Hebrew,
            Face::Arabic,
        ],
    }
}

/// System faces tried for a CJK character, its own script's slot first. Loading is lazy, so a
/// machine without CJK text never reads a system font.
fn os_chain(c: char, bold: bool) -> &'static [Face] {
    match (os_fonts::slot_for(c), bold) {
        (Some(Slot::Han), false) => &[Face::Cjk, Face::Hangul],
        (Some(Slot::Hangul), false) => &[Face::Hangul, Face::Cjk],
        // A bold face only exists when the machine has a real one; the regular faces follow so
        // bold text without a bold font looks as it always did.
        (Some(Slot::Han), true) => &[Face::CjkBold, Face::Cjk, Face::HangulBold, Face::Hangul],
        (Some(Slot::Hangul), true) => &[Face::HangulBold, Face::Hangul, Face::CjkBold, Face::Cjk],
        (None, _) => &[],
    }
}

/// A resolved glyph.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Glyph {
    pub face: Face,
    /// Glyph id in the original font (0 = `.notdef`).
    pub gid: u16,
    /// Advance in 1/1000 em.
    pub advance: f32,
}

/// Characters that are not drawn at all (controls, bidi marks, joiners).
fn is_invisible(c: char) -> bool {
    c.is_control() && c != '\t'
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}' | '\u{FEFF}')
}

/// Resolve `c` for a style. `None` for characters that draw nothing.
pub(crate) fn glyph(style: impl Into<Style>, c: char) -> Option<Glyph> {
    let c = if c == '\t' { ' ' } else { c };
    if is_invisible(c) {
        return None;
    }
    let style = style.into();
    let faces = chain(style);
    for &face in faces {
        if let Some(id) = face.parsed().glyph_index(c) {
            return Some(resolved(face, id.0));
        }
    }
    for &face in os_chain(c, matches!(style, Style::SansBold | Style::SerifBold)) {
        if !face.available() {
            continue;
        }
        if let Some(id) = face.parsed().glyph_index(c) {
            return Some(resolved(face, id.0));
        }
    }
    Some(resolved(faces[0], 0))
}

/// The advance of `gid` in `face`, in 1/1000 em.
pub(crate) fn advance(face: Face, gid: u16) -> f32 {
    resolved(face, gid).advance
}

pub(crate) fn resolved(face: Face, gid: u16) -> Glyph {
    let units = face
        .parsed()
        .glyph_hor_advance(ttf_parser::GlyphId(gid))
        .unwrap_or(0);
    Glyph {
        face,
        gid,
        advance: f32::from(units) * 1000.0 / face.units_per_em(),
    }
}

/// Width of `text` in points at `size`. Text with Arabic letters is measured shaped.
pub(crate) fn text_width(style: impl Into<Style>, text: &str, size: f32) -> f32 {
    let style = style.into();
    if crate::shaping::needs_shaping(text) {
        return crate::pdf_fonts::shape(style, text).width() * size / 1000.0;
    }
    text.chars()
        .filter_map(|c| glyph(style, c))
        .map(|g| g.advance)
        .sum::<f32>()
        * size
        / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fonts_parse_and_cover_scripts() {
        for f in Face::ALL {
            assert!(f.parsed().number_of_glyphs() > 10, "{f:?}");
        }
        for c in ['A', 'é', 'Š', 'Æ', 'Ж', 'α', 'א'] {
            let g = glyph(Style::Sans, c).unwrap();
            assert_ne!(g.gid, 0, "{c} has a glyph");
        }
        assert_eq!(glyph(Style::Sans, 'א').unwrap().face, Face::Hebrew);
        assert_eq!(glyph(Style::SansBold, 'א').unwrap().face, Face::HebrewBold);
        assert_eq!(glyph(Style::Sans, 'ب').unwrap().face, Face::Arabic);
        assert_eq!(glyph(Style::SansBold, 'ب').unwrap().face, Face::ArabicBold);
        assert_eq!(glyph(Style::SerifBold, 'ب').unwrap().face, Face::ArabicBold);
        // CJK comes from a system font when the machine has one, else it stays `.notdef`.
        let han = glyph(Style::Sans, '日').unwrap();
        if Face::Cjk.available() {
            assert_eq!((han.face, han.gid != 0), (Face::Cjk, true));
        } else {
            assert_eq!(han.gid, 0, "CJK is not covered");
        }
        // Bold CJK uses the bold system face when the machine has one, else the regular one.
        let bold = glyph(Style::SansBold, '日').unwrap();
        if Face::CjkBold.available() {
            eprintln!("CJK bold face: {}", Face::CjkBold.ps_name());
            assert_eq!((bold.face, bold.gid != 0), (Face::CjkBold, true));
        } else if Face::Cjk.available() {
            assert_eq!((bold.face, bold.gid != 0), (Face::Cjk, true));
        }
        assert_eq!(glyph(Style::Sans, '日').unwrap().face, han.face);
        assert!(glyph(Style::Sans, '\u{200F}').is_none());
        for style in [Style::Serif, Style::SerifItalic] {
            for c in ['A', 'é', 'Ж', 'α', '\u{2212}'] {
                let g = glyph(style, c).unwrap();
                assert_eq!((g.face, g.gid != 0), (style.primary(), true), "{c}");
            }
            assert_eq!(glyph(style, 'א').unwrap().face, Face::Hebrew);
        }
    }

    #[test]
    fn widths_are_proportional() {
        let w = text_width(Style::Sans, "AAAA", 10.0);
        assert!(w > 20.0 && w < 40.0, "{w}");
        assert!(text_width(Style::SansBold, "AAAA", 10.0) > w);
    }
}
