//! Type0/CIDFontType2 (Identity-H) font embedding: per-face glyph subsetting, `/W` widths and a
//! `ToUnicode` CMap so the text stays selectable and searchable.

use crate::bidi::{unreverse, visual_order};
use crate::fonts::{glyph, Face, Style};
use crate::shaping::{is_joiner, shape_rtl, ShapedGlyph};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use subsetter::GlyphRemapper;

/// A run of glyphs from one face, in left-to-right display order.
pub(crate) struct Run {
    pub face: Face,
    pub glyphs: Vec<ShapedGlyph>,
}

/// A shaped line: runs in display order plus whether display order differs from logical order.
pub(crate) struct Shaped {
    pub runs: Vec<Run>,
    pub reordered: bool,
}

impl Shaped {
    /// Total advance in 1/1000 em.
    pub fn width(&self) -> f32 {
        self.runs
            .iter()
            .flat_map(|r| &r.glyphs)
            .map(|g| g.advance)
            .sum()
    }
}

fn plain(gid: u16, c: char, advance: f32) -> ShapedGlyph {
    ShapedGlyph {
        gid,
        text: c.to_string(),
        advance,
        dx: 0.0,
        dy: 0.0,
    }
}

/// Replace a run of a shaped (Arabic) face by the shaper's output. The run holds display-order
/// characters; the shaper wants them logical.
fn reshape(run: &mut Run) {
    let visual: Vec<char> = run.glyphs.iter().flat_map(|g| g.text.chars()).collect();
    if let Some(glyphs) = shape_rtl(run.face, &unreverse(&visual)) {
        run.glyphs = glyphs;
    } else {
        run.glyphs
            .retain(|g| g.gid != 0 || !g.text.chars().all(is_joiner));
    }
}

/// Map text to glyph runs (bidi reordering, per-style fallback faces, Arabic shaping).
pub(crate) fn shape(style: impl Into<Style>, text: &str) -> Shaped {
    let font = style.into();
    let visual = visual_order(text);
    let reordered = visual.is_some();
    let chars = visual.unwrap_or_else(|| text.chars().collect());
    let mut runs: Vec<Run> = Vec::new();
    for c in chars {
        // Joiners draw nothing but steer Arabic joining, so they stay with an Arabic run.
        if is_joiner(c) {
            if let Some(run) = runs.last_mut().filter(|r| r.face.shaped()) {
                run.glyphs.push(plain(0, c, 0.0));
            }
            continue;
        }
        let Some(g) = glyph(font, c) else { continue };
        match runs.last_mut() {
            Some(run) if run.face == g.face => run.glyphs.push(plain(g.gid, c, g.advance)),
            _ => runs.push(Run {
                face: g.face,
                glyphs: vec![plain(g.gid, c, g.advance)],
            }),
        }
    }
    for run in runs.iter_mut().filter(|r| r.face.shaped()) {
        reshape(run);
    }
    Shaped { runs, reordered }
}

/// Glyphs used per face (with the text each stands for), then frozen into subset plans.
pub(crate) struct Usage {
    used: Vec<BTreeMap<u16, String>>,
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
            for g in &run.glyphs {
                let text = map.entry(g.gid).or_default();
                if text.is_empty() {
                    text.clone_from(&g.text);
                }
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
    used: BTreeMap<u16, String>,
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
    /// CFF outlines (embedded as `CIDFontType0` / `FontFile3`), else TrueType.
    pub cff: bool,
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

fn utf16_hex(text: &str) -> String {
    text.encode_utf16().map(|u| format!("{u:04X}")).collect()
}

fn cmap(plan: &SubsetPlan) -> String {
    let mut entries: Vec<(u16, &str)> = plan
        .used
        .iter()
        .filter(|(gid, text)| **gid != 0 && !text.is_empty())
        .map(|(gid, text)| (plan.cid(*gid), text.as_str()))
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
            let _ = writeln!(s, "<{cid:04X}> <{}>", utf16_hex(c));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// Subset the face and build its widths and `ToUnicode` data. `None` if subsetting fails.
pub(crate) fn build(plan: &SubsetPlan) -> Option<FontParts> {
    let face = plan.face;
    let subset = match subsetter::subset(face.data(), face.collection_index(), &plan.remap) {
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
        cff: parsed.tables().cff.is_some() && parsed.tables().glyf.is_none(),
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

    /// Whether any embedded subset has CFF outlines.
    pub fn has_cff(&self) -> bool {
        self.faces.iter().flatten().any(|f| f.parts.cff)
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
            if !ops.is_empty() {
                ops.push(' ');
            }
            let _ = write!(ops, "/{} {size:.2} Tf ", run.face.resource());
            if run.face.shaped() {
                ops.push_str(&positioned(font, run, size));
            } else {
                let hex: String = run
                    .glyphs
                    .iter()
                    .map(|g| format!("{:04X}", font.plan.cid(g.gid)))
                    .collect();
                let _ = write!(ops, "<{hex}> Tj");
            }
        }
        (!ops.is_empty()).then_some(EncodedText {
            ops,
            reordered: shaped.reordered,
        })
    }
}

/// A shaped run as `TJ` arrays: kerning numbers move each glyph from where its `/W` width
/// leaves the pen to where the shaper put it, and `Ts` (text rise) carries vertical mark offsets.
fn positioned(font: &EmbeddedFont, run: &Run, size: f32) -> String {
    let face = run.face;
    let mut out = String::new();
    let mut array = String::new();
    let mut rise = 0.0_f32;
    // Pending horizontal move, in 1/1000 em (positive = right).
    let mut shift = run.glyphs.first().map_or(0.0, |g| g.dx);
    let flush = |out: &mut String, array: &mut String| {
        if !array.is_empty() {
            let _ = write!(out, "[{array}] TJ ");
            array.clear();
        }
    };
    for (i, g) in run.glyphs.iter().enumerate() {
        let dy = (g.dy * size / 1000.0 * 100.0).round() / 100.0;
        if dy != rise {
            flush(&mut out, &mut array);
            let _ = write!(out, "{dy:.2} Ts ");
            rise = dy;
        }
        let kern = shift.round();
        if kern != 0.0 {
            let _ = write!(array, "{} ", -kern);
        }
        let _ = write!(array, "<{:04X}> ", font.plan.cid(g.gid));
        let next_dx = run.glyphs.get(i + 1).map_or(0.0, |n| n.dx);
        // The viewer advances by the rounded `/W` width, which `build` writes per glyph.
        let width = crate::fonts::advance(face, g.gid).round();
        shift = g.advance - width - g.dx + next_dx + (shift - kern);
    }
    let kern = shift.round();
    if kern != 0.0 {
        let _ = write!(array, "{} ", -kern);
    }
    flush(&mut out, &mut array);
    if rise != 0.0 {
        out.push_str("0.00 Ts ");
    }
    out.trim_end().to_string()
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

    #[test]
    fn mixed_line_keeps_latin_left_and_shapes_the_arabic_word() {
        // Logical: Latin word, space, seen-lam-alef-meem.
        let shaped = shape(Style::Sans, "Ægir \u{633}\u{644}\u{627}\u{645}");
        assert!(shaped.reordered);
        let faces: Vec<Face> = shaped.runs.iter().map(|r| r.face).collect();
        assert_eq!(faces, [Face::Sans, Face::Arabic]);
        let latin: String = shaped.runs[0]
            .glyphs
            .iter()
            .map(|g| g.text.as_str())
            .collect();
        assert_eq!(latin, "Ægir ");
        // The Arabic run is in display order: the last letter (meem) is leftmost.
        let arabic: Vec<&str> = shaped.runs[1]
            .glyphs
            .iter()
            .map(|g| g.text.as_str())
            .collect();
        assert_eq!(arabic, ["\u{645}", "\u{627}", "\u{644}", "\u{633}"]);
        let isolated = crate::fonts::glyph(Style::Sans, '\u{633}').unwrap().gid;
        assert_ne!(
            shaped.runs[1].glyphs[3].gid, isolated,
            "seen takes its initial form"
        );
        // Layout measures the shaped advances.
        let w = crate::fonts::text_width(Style::Sans, "\u{633}\u{644}\u{627}\u{645}", 10.0);
        let shaped_w = shape(Style::Sans, "\u{633}\u{644}\u{627}\u{645}").width() / 100.0;
        assert!((w - shaped_w).abs() < 1e-3 && w > 5.0, "{w} {shaped_w}");
    }

    #[test]
    fn shaped_marks_are_positioned_with_kerning_and_rise() {
        // beh + fatha: dot and fatha are zero-advance marks with offsets.
        let text = "\u{628}\u{64E}";
        let mut usage = Usage::default();
        usage.record_text(Style::Sans, text);
        let fonts = EmbeddedFonts::embed(usage);
        let enc = fonts.encode(Style::Sans, text, 10.0).unwrap();
        assert!(enc.reordered);
        assert!(enc.ops.starts_with("/F9 10.00 Tf "), "{}", enc.ops);
        assert!(
            enc.ops.contains("] TJ") && enc.ops.contains(" Ts "),
            "{}",
            enc.ops
        );
        // Text rise is graphics state that outlives ET, so the run always ends at rise 0.
        let parts: Vec<&str> = enc.ops.split(" Ts").collect();
        let last_rise = parts[parts.len() - 2].rsplit(' ').next();
        assert_eq!(last_rise, Some("0.00"), "rise is reset: {}", enc.ops);
        assert!(!enc.ops.contains("<0000>"), "{}", enc.ops);
    }

    #[test]
    fn unshaped_text_is_unchanged_by_the_arabic_path() {
        let mut usage = Usage::default();
        usage.record_text(Style::Sans, "Ab");
        let fonts = EmbeddedFonts::embed(usage);
        let enc = fonts.encode(Style::Sans, "Ab", 10.0).unwrap();
        assert_eq!(enc.ops, "/F1 10.00 Tf <00010002> Tj");
        assert!(!enc.reordered && !fonts.has_cff());
    }
}
