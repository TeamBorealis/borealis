//! OpenType shaping of Arabic runs (rustybuzz): contextual forms (isolated, initial, medial,
//! final), lam-alef and other ligatures, and mark placement. Other scripts keep the plain
//! one-glyph-per-character mapping.

use crate::fonts::Face;
use std::sync::OnceLock;

/// Whether `c` belongs to an Arabic block (letters, marks, digits, presentation forms).
pub(crate) fn is_arabic(c: char) -> bool {
    matches!(
        u32::from(c),
        0x0600..=0x06FF | 0x0750..=0x077F | 0x0870..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
    )
}

/// Whether `text` has anything the shaper must see (layout then measures it shaped).
pub(crate) fn needs_shaping(text: &str) -> bool {
    text.chars().any(is_arabic)
}

/// Zero-width joiner / non-joiner: invisible, but they control Arabic joining.
pub(crate) fn is_joiner(c: char) -> bool {
    matches!(c, '\u{200C}' | '\u{200D}')
}

/// One output glyph of the shaper, in display order. Metrics are in 1/1000 em.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShapedGlyph {
    pub gid: u16,
    /// The characters this glyph stands for (several for a ligature, empty for the extra glyphs
    /// of a decomposed character).
    pub text: String,
    pub advance: f32,
    pub dx: f32,
    pub dy: f32,
}

fn rb_face(face: Face) -> Option<&'static rustybuzz::Face<'static>> {
    static FACES: OnceLock<Vec<(Face, rustybuzz::Face<'static>)>> = OnceLock::new();
    FACES
        .get_or_init(|| {
            Face::EMBEDDED
                .iter()
                .filter(|f| f.shaped())
                .filter_map(|f| Some((*f, rustybuzz::Face::from_slice(f.data(), 0)?)))
                .collect()
        })
        .iter()
        .find(|(f, _)| *f == face)
        .map(|(_, rb)| rb)
}

/// Shape a right-to-left run given in logical order. The glyphs come back left to right.
/// `None` when `face` has no shaper (it is not an Arabic face).
pub(crate) fn shape_rtl(face: Face, logical: &str) -> Option<Vec<ShapedGlyph>> {
    let rb = rb_face(face)?;
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(logical);
    buf.set_direction(rustybuzz::Direction::RightToLeft);
    buf.set_script(rustybuzz::script::ARABIC);
    let out = rustybuzz::shape(rb, &[], buf);
    let scale = 1000.0 / face.units_per_em();
    // Clusters are byte offsets into `logical`; a cluster spans up to the next cluster start.
    let mut starts: Vec<usize> = out
        .glyph_infos()
        .iter()
        .map(|i| i.cluster as usize)
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let cluster_text = |start: usize| -> &str {
        let end = starts
            .iter()
            .find(|s| **s > start)
            .copied()
            .unwrap_or(logical.len());
        logical.get(start..end).unwrap_or("")
    };
    let mut named = std::collections::BTreeSet::new();
    let glyphs = out
        .glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(info, pos)| {
            let cluster = info.cluster as usize;
            let text = if named.insert(cluster) {
                cluster_text(cluster).to_string()
            } else {
                String::new()
            };
            ShapedGlyph {
                gid: u16::try_from(info.glyph_id).unwrap_or(0),
                text,
                advance: pos.x_advance as f32 * scale,
                dx: pos.x_offset as f32 * scale,
                dy: pos.y_offset as f32 * scale,
            }
        })
        .collect();
    Some(glyphs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gid(face: Face, c: char) -> u16 {
        face.parsed().glyph_index(c).map_or(0, |g| g.0)
    }

    /// Glyph names of the shaped output (the pinned Noto Sans Arabic keeps its `post` names).
    fn names(text: &str) -> Vec<String> {
        shape_rtl(Face::Arabic, text)
            .unwrap()
            .iter()
            .map(|g| {
                let name = Face::Arabic.parsed().glyph_name(ttf_parser::GlyphId(g.gid));
                name.unwrap_or("?").to_string()
            })
            .collect()
    }

    #[test]
    fn joining_letters_take_contextual_forms() {
        // meem meem meem: final, medial, initial forms, left to right (the last letter first).
        assert_eq!(
            names("\u{645}\u{645}\u{645}"),
            ["uni0645.fina", "uni0645.medi", "uni0645.init"]
        );
        // A lone letter keeps its nominal (isolated) glyph.
        let lone = shape_rtl(Face::Arabic, "\u{645}").unwrap();
        assert_eq!(lone.len(), 1);
        assert_eq!(lone[0].gid, gid(Face::Arabic, '\u{645}'));
        // A word: seen-initial, lam-medial, alef-final in the lam-alef ligature forms, isolated
        // meem after the non-joining alef.
        assert_eq!(
            names("\u{633}\u{644}\u{627}\u{645}"),
            [
                "uni0645",
                "uni0627.fina.rlig.2",
                "uni0644.medi.rlig",
                "uni0633.init"
            ]
        );
    }

    #[test]
    fn lam_alef_uses_the_required_ligature_forms() {
        // This font draws lam-alef as a contextual pair (`rlig`), not one precomposed glyph.
        let g = shape_rtl(Face::Arabic, "\u{644}\u{627}").unwrap();
        assert_eq!(
            names("\u{644}\u{627}"),
            ["uni0627.fina.rlig", "uni0644.init.rlig"]
        );
        assert_ne!(g[0].gid, gid(Face::Arabic, '\u{627}'));
        let texts: Vec<&str> = g.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(
            texts,
            ["\u{627}", "\u{644}"],
            "each glyph maps back to its letter"
        );
    }

    #[test]
    fn marks_attach_with_zero_advance() {
        // beh + fatha: one cluster; the font draws beh as a dotless body plus a dot, and both
        // the dot and the fatha are zero-advance marks placed by offsets.
        let g = shape_rtl(Face::Arabic, "\u{628}\u{64E}").unwrap();
        let fatha = gid(Face::Arabic, '\u{64E}');
        let mark = g.iter().find(|x| x.gid == fatha).expect("fatha glyph");
        assert!(mark.advance.abs() < 0.01 && mark.dy.abs() > 0.0, "{g:?}");
        assert!(
            g.iter().any(|x| x.advance > 100.0),
            "the body advances: {g:?}"
        );
        let texts: Vec<&str> = g.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(
            texts.iter().filter(|t| !t.is_empty()).count(),
            1,
            "{texts:?}"
        );
        assert!(texts.contains(&"\u{628}\u{64E}"), "{texts:?}");
    }

    #[test]
    fn detection() {
        assert!(needs_shaping("Ægir \u{633}\u{644}\u{627}\u{645}"));
        assert!(!needs_shaping("אלון Ægir"));
        assert!(shape_rtl(Face::Sans, "abc").is_none());
    }
}
