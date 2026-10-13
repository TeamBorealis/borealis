//! Type0/CIDFontType2 (Identity-H) font embedding: per-face glyph subsetting, `/W` widths and a
//! `ToUnicode` CMap so the text stays selectable and searchable.

use crate::bidi::visual_order;
use crate::fonts::{glyph, Face, Style};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use subsetter::GlyphRemapper;

/// A run of glyphs from one face, in left-to-right display order.
pub(crate) struct Run {
    pub face: Face,
    /// (original glyph id, source character)
    pub glyphs: Vec<(u16, char)>,
}

/// A shaped line: runs in display order plus whether display order differs from logical order.
pub(crate) struct Shaped {
    pub runs: Vec<Run>,
    pub reordered: bool,
}

/// Map text to glyph runs (bidi reordering, per-style fallback faces).
pub(crate) fn shape(style: impl Into<Style>, text: &str) -> Shaped {
    let font = style.into();
    let visual = visual_order(text);
    let reordered = visual.is_some();
    let chars = visual.unwrap_or_else(|| text.chars().collect());
    let mut runs: Vec<Run> = Vec::new();
    for c in chars {
        let Some(g) = glyph(font, c) else { continue };
        match runs.last_mut() {
            Some(run) if run.face == g.face => run.glyphs.push((g.gid, c)),
            _ => runs.push(Run {
                face: g.face,
                glyphs: vec![(g.gid, c)],
            }),
        }
    }
    Shaped { runs, reordered }
}

/// Glyphs used per face, then frozen into subset plans.
pub(crate) struct Usage {
    used: Vec<BTreeMap<u16, char>>,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            used: vec![BTreeMap::new(); Face::ALL.len()],
        }
    }
}

impl Usage {
    /// Record the glyphs that `text` needs in `style`.
    pub fn record_text(&mut self, style: Style, text: &str) {
        self.record(&shape(style, text));
    }

    pub fn record(&mut self, shaped: &Shaped) {
        for run in &shaped.runs {
            let map = &mut self.used[run.face.index()];
            for (gid, c) in &run.glyphs {
                map.entry(*gid).or_insert(*c);
            }
        }
    }

    pub fn plan(self) -> Vec<Option<SubsetPlan>> {
        Face::ALL
            .iter()
            .zip(self.used)
            .map(|(face, used)| {
                (!used.is_empty()).then(|| {
                    let mut remap = GlyphRemapper::new();
                    remap.remap(0);
                    for gid in used.keys().filter(|g| **g != 0) {
                        remap.remap(*gid);
                    }
                    SubsetPlan {
                        face: *face,
                        used,
                        remap,
                    }
                })
            })
            .collect()
    }
}

/// The glyph set chosen for one face.
pub(crate) struct SubsetPlan {
    pub face: Face,
    used: BTreeMap<u16, char>,
    remap: GlyphRemapper,
}

impl SubsetPlan {
    /// CID (= glyph id in the subset) for an original glyph id.
    pub fn cid(&self, gid: u16) -> u16 {
        self.remap.get(gid).unwrap_or(0)
    }
}

/// Everything needed to write one embedded font.
pub(crate) struct FontParts {
    pub base_font: String,
    pub subset: Vec<u8>,
    pub widths: String,
    pub to_unicode: String,
    pub bbox: [i32; 4],
    pub ascent: i32,
    pub descent: i32,
    pub cap_height: i32,
    pub italic_angle: f32,
}

fn scaled(v: i16, upem: f32) -> i32 {
    (f32::from(v) * 1000.0 / upem).round() as i32
}

fn tag(face: Face, gids: impl Iterator<Item = u16>) -> String {
    // FNV-1a over the face and glyph set gives a stable six-letter subset tag.
    let mut h: u32 = 0x811c_9dc5;
    for b in face.ps_name().bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    for g in gids {
        for b in g.to_be_bytes() {
            h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
        }
    }
    (0..6)
        .map(|i| char::from(b'A' + u8::try_from((h >> (i * 5)) % 26).unwrap_or(0)))
        .collect()
}

fn utf16_hex(c: char) -> String {
    let mut buf = [0u16; 2];
    c.encode_utf16(&mut buf)
        .iter()
        .map(|u| format!("{u:04X}"))
        .collect()
}

fn cmap(plan: &SubsetPlan) -> String {
    let mut entries: Vec<(u16, char)> = plan
        .used
        .iter()
        .filter(|(gid, _)| **gid != 0)
        .map(|(gid, c)| (plan.cid(*gid), *c))
        .collect();
    entries.sort_unstable();
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (cid, c) in chunk {
            let _ = writeln!(s, "<{cid:04X}> <{}>", utf16_hex(*c));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// Subset the face and build its widths and `ToUnicode` data. `None` if subsetting fails.
pub(crate) fn build(plan: &SubsetPlan) -> Option<FontParts> {
    let face = plan.face;
    let subset = match subsetter::subset(face.data(), 0, &plan.remap) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("font subsetting failed for {}: {e}", face.ps_name());
            return None;
        }
    };
    let parsed = face.parsed();
    let upem = face.units_per_em();
    // Widths indexed by CID; every used glyph has a contiguous CID starting at 0.
    let mut widths: BTreeMap<u16, i32> = BTreeMap::new();
    widths.insert(0, 1000);
    for gid in plan.used.keys() {
        let adv = parsed
            .glyph_hor_advance(ttf_parser::GlyphId(*gid))
            .unwrap_or(0);
        widths.insert(
            plan.cid(*gid),
            (f32::from(adv) * 1000.0 / upem).round() as i32,
        );
    }
    let mut w = String::from("[ ");
    let mut next: Option<u16> = None;
    for (cid, adv) in &widths {
        if next != Some(*cid) {
            if next.is_some() {
                w.push_str("] ");
            }
            let _ = write!(w, "{cid} [");
        }
        let _ = write!(w, " {adv}");
        next = cid.checked_add(1);
    }
    w.push_str(" ] ]");
    let bb = parsed.global_bounding_box();
    Some(FontParts {
        base_font: format!(
            "{}+{}",
            tag(face, plan.used.keys().copied()),
            face.ps_name()
        ),
        subset,
        widths: w,
        to_unicode: cmap(plan),
        bbox: [
            scaled(bb.x_min, upem),
            scaled(bb.y_min, upem),
            scaled(bb.x_max, upem),
            scaled(bb.y_max, upem),
        ],
        ascent: scaled(parsed.ascender(), upem),
        descent: scaled(parsed.descender(), upem),
        cap_height: parsed.capital_height().map_or(700, |h| scaled(h, upem)),
        italic_angle: parsed.italic_angle(),
    })
}

/// A font that was subset successfully.
pub(crate) struct EmbeddedFont {
    pub plan: SubsetPlan,
    pub parts: FontParts,
}

/// The subset faces of one document, indexed by [`Face::index`].
pub(crate) struct EmbeddedFonts {
    faces: Vec<Option<EmbeddedFont>>,
}

/// Text encoded as content-stream operators (`/F1 9.00 Tf <...> Tj` per run).
pub(crate) struct EncodedText {
    pub ops: String,
    /// Display order differs from reading order (wrap in `/ActualText`).
    pub reordered: bool,
}

impl EmbeddedFonts {
    /// Subset every face that `usage` touched; faces that fail to subset are dropped.
    pub fn embed(usage: Usage) -> Self {
        let faces = usage
            .plan()
            .into_iter()
            .map(|plan| {
                let plan = plan?;
                let parts = build(&plan)?;
                Some(EmbeddedFont { plan, parts })
            })
            .collect();
        Self { faces }
    }

    pub fn get(&self, face: Face) -> Option<&EmbeddedFont> {
        self.faces.get(face.index()).and_then(Option::as_ref)
    }

    /// `text` as show-text operators in the embedded subsets. `None` when nothing is drawable.
    pub fn encode(&self, style: Style, text: &str, size: f32) -> Option<EncodedText> {
        let shaped = shape(style, text);
        let mut ops = String::new();
        for run in &shaped.runs {
            let Some(font) = self.get(run.face) else {
                continue;
            };
            let hex: String = run
                .glyphs
                .iter()
                .map(|(gid, _)| format!("{:04X}", font.plan.cid(*gid)))
                .collect();
            if !ops.is_empty() {
                ops.push(' ');
            }
            let _ = write!(ops, "/{} {size:.2} Tf <{hex}> Tj", run.face.resource());
        }
        (!ops.is_empty()).then_some(EncodedText {
            ops,
            reordered: shaped.reordered,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_uses_subset_cids_and_fallback_faces() {
        let mut usage = Usage::default();
        usage.record_text(Style::SerifBold, "Aא");
        let fonts = EmbeddedFonts::embed(usage);
        let enc = fonts.encode(Style::SerifBold, "Aא", 10.0).unwrap();
        // CID 0 is .notdef, so the first used glyph of each face is CID 1.
        assert_eq!(enc.ops, "/F4 10.00 Tf <0001> Tj /F6 10.00 Tf <0001> Tj");
        assert!(fonts.get(Face::Sans).is_none());
    }
}
