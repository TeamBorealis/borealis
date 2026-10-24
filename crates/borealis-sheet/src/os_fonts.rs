//! CJK fallback faces loaded from the operating system at render time.
//!
//! Full CJK fonts are tens of megabytes, so none is compiled in. The first time a sheet needs a
//! Han/kana or Hangul character that no embedded face covers, one installed font per slot is
//! loaded (first usable candidate in a fixed priority order, so the choice is stable for a given
//! machine) and later subset like the embedded faces. Without a usable font the characters keep
//! drawing `.notdef`, as before.

use crate::os_font_paths as paths;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use ttf_parser::Face as Parsed;

/// Which script family a system face is chosen for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// Han ideographs, kana, bopomofo and CJK punctuation.
    Han,
    /// Hangul.
    Hangul,
}

/// A system font file chosen for a slot.
pub(crate) struct OsFont {
    pub data: &'static [u8],
    /// Face index inside a TrueType collection (0 for a plain font file).
    pub index: u32,
    /// PostScript name, reduced to characters valid in a PDF name.
    pub ps_name: String,
    pub path: PathBuf,
}

/// A loaded font and its parsed face.
pub(crate) struct Loaded {
    pub font: OsFont,
    pub parsed: Parsed<'static>,
}

/// Characters the system CJK faces are consulted for.
pub(crate) fn slot_for(c: char) -> Option<Slot> {
    match u32::from(c) {
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => Some(Slot::Hangul),
        0x2E80..=0x2FDF
        | 0x2FF0..=0x303F
        | 0x3040..=0x30FF
        | 0x3100..=0x312F
        | 0x3190..=0x31FF
        | 0x3200..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFF => Some(Slot::Han),
        _ => None,
    }
}

/// A character every acceptable font of the slot must have.
fn probe(slot: Slot) -> char {
    match slot {
        Slot::Han => '\u{4E00}',
        Slot::Hangul => '\u{D55C}',
    }
}

#[cfg(target_os = "macos")]
fn candidates(slot: Slot) -> Vec<PathBuf> {
    paths::macos_candidates(slot)
}

#[cfg(target_os = "ios")]
fn candidates(_slot: Slot) -> Vec<PathBuf> {
    paths::ios_candidates()
}

#[cfg(target_os = "windows")]
fn candidates(slot: Slot) -> Vec<PathBuf> {
    let windir =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let mut dirs = vec![windir.join("Fonts")];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
    }
    paths::windows_candidates(slot, &dirs)
}

#[cfg(target_os = "android")]
fn candidates(_slot: Slot) -> Vec<PathBuf> {
    paths::android_candidates()
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn candidates(_slot: Slot) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let mut found = Vec::new();
    for root in paths::linux_roots(home.as_deref(), xdg.as_deref()) {
        paths::scan_font_dir(&root, paths::SCAN_DEPTH, &mut found);
    }
    paths::linux_candidates(&found)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn candidates(_slot: Slot) -> Vec<PathBuf> {
    Vec::new()
}

/// Letters, digits and `-` of the font's PostScript name, else of its file name (PDF names
/// allow more, but a font name never needs it).
fn clean_ps_name(parsed: &Parsed<'_>, path: &Path) -> String {
    let raw = parsed
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
        .find_map(|n| n.to_string())
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let name: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(63)
        .collect();
    if name.is_empty() {
        "SystemCJK".into()
    } else {
        name
    }
}

/// Weight the regular face aims for (OS/2 `usWeightClass`).
const REGULAR_WEIGHT: u16 = 400;

/// A usable face inside a font file, with what the chooser ranks on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FaceInfo {
    /// Face index inside the file (0 for a plain font).
    index: u32,
    /// 0 when the face names the slot's preferred language, else 1.
    rank: u8,
    weight: u16,
}

/// Name words that mark a face as made for the slot's default language: Simplified Chinese for
/// Han (a collection such as Noto Sans CJK lists the Japanese face first, but nothing in a name
/// to render says which language it is) and Korean for Hangul.
fn language_tags(slot: Slot) -> &'static [&'static str] {
    match slot {
        Slot::Han => &["sc", "gb", "cn", "hans", "simplified"],
        Slot::Hangul => &["kr", "kor", "korean"],
    }
}

/// 0 when any of `names` has a word from the slot's language tags ("Noto Sans CJK SC Regular"),
/// else 1.
fn tag_rank(names: &[String], slot: Slot) -> u8 {
    let tags = language_tags(slot);
    let tagged = names.iter().any(|n| {
        n.split(|c: char| !c.is_alphanumeric())
            .any(|w| tags.iter().any(|t| w.eq_ignore_ascii_case(t)))
    });
    u8::from(!tagged)
}

fn face_names(parsed: &Parsed<'_>) -> Vec<String> {
    use ttf_parser::name_id::{FAMILY, FULL_NAME, TYPOGRAPHIC_FAMILY};
    parsed
        .names()
        .into_iter()
        .filter(|n| [FAMILY, FULL_NAME, TYPOGRAPHIC_FAMILY].contains(&n.name_id))
        .filter_map(|n| n.to_string())
        .collect()
}

/// Whether a parsed face can stand in for a slot: outlines the PDF writer embeds, the slot's
/// probe character, and a licence that allows embedding a subset.
fn usable(parsed: &Parsed<'_>, probe: char) -> bool {
    let tables = parsed.tables();
    let outlines = tables.glyf.is_some() || tables.cff.is_some();
    let licensed = parsed.permissions() != Some(ttf_parser::Permissions::Restricted)
        && parsed.is_subsetting_allowed();
    outlines && licensed && parsed.glyph_index(probe).is_some()
}

/// Every usable face of a font file or collection. Unparseable faces are skipped.
fn usable_faces(data: &[u8], slot: Slot, probe: char) -> Vec<FaceInfo> {
    let count = ttf_parser::fonts_in_collection(data).unwrap_or(1);
    (0..count)
        .filter_map(|index| {
            let parsed = Parsed::parse(data, index).ok()?;
            usable(&parsed, probe).then(|| FaceInfo {
                index,
                rank: tag_rank(&face_names(&parsed), slot),
                weight: parsed.weight().to_number(),
            })
        })
        .collect()
}

/// The regular face: preferred language first, then the weight closest to 400, then the lowest
/// index (so the choice is deterministic).
fn pick_regular(faces: &[FaceInfo]) -> Option<FaceInfo> {
    faces
        .iter()
        .copied()
        .min_by_key(|f| (f.rank, f.weight.abs_diff(REGULAR_WEIGHT), f.index))
}

type SharedFiles = Mutex<Vec<(PathBuf, &'static [u8])>>;

/// Font files already read, shared between the slots (one file often serves Han and Hangul).
fn shared() -> &'static SharedFiles {
    static SHARED: OnceLock<SharedFiles> = OnceLock::new();
    SHARED.get_or_init(|| Mutex::new(Vec::new()))
}

fn cached(path: &Path) -> Option<&'static [u8]> {
    let files = shared().lock().ok()?;
    files.iter().find(|(p, _)| p == path).map(|(_, d)| *d)
}

/// Keep an accepted font file for the life of the process.
fn remember(path: &Path, data: Vec<u8>) -> &'static [u8] {
    let data: &'static [u8] = Box::leak(data.into_boxed_slice());
    if let Ok(mut files) = shared().lock() {
        files.push((path.to_path_buf(), data));
    }
    data
}

/// Describe face `info` of `data`, read from `path`.
fn font_at(data: &'static [u8], path: &Path, info: FaceInfo) -> Option<OsFont> {
    let parsed = Parsed::parse(data, info.index).ok()?;
    Some(OsFont {
        data,
        index: info.index,
        ps_name: clean_ps_name(&parsed, path),
        path: path.to_path_buf(),
    })
}

/// Read `path` and keep it when one of its faces is usable for `slot`; the face is chosen by
/// [`pick_regular`].
fn open(path: &Path, slot: Slot) -> Option<OsFont> {
    let probe = probe(slot);
    let data = match cached(path) {
        Some(data) => data,
        None => {
            let owned = std::fs::read(path).ok()?;
            if usable_faces(&owned, slot, probe).is_empty() {
                return None;
            }
            remember(path, owned)
        }
    };
    let info = pick_regular(&usable_faces(data, slot, probe))?;
    font_at(data, path, info)
}

fn find(slot: Slot) -> Option<OsFont> {
    let found = candidates(slot).iter().find_map(|p| open(p, slot));
    match &found {
        Some(f) => tracing::debug!(
            "sheet: system CJK font for {slot:?}: {} (face {})",
            f.path.display(),
            f.index
        ),
        None => tracing::warn!(
            "sheet: no system font for {slot:?} text; those characters render as missing glyphs"
        ),
    }
    found
}

/// The system face for `slot`, loading it on first use. `None` when the machine has none.
pub(crate) fn loaded(slot: Slot) -> Option<&'static Loaded> {
    static HAN: OnceLock<Option<Loaded>> = OnceLock::new();
    static HANGUL: OnceLock<Option<Loaded>> = OnceLock::new();
    let cell = match slot {
        Slot::Han => &HAN,
        Slot::Hangul => &HANGUL,
    };
    cell.get_or_init(|| {
        let font = find(slot)?;
        // `open` already parsed this face once, so this cannot fail in practice.
        let parsed = Parsed::parse(font.data, font.index).ok()?;
        Some(Loaded { font, parsed })
    })
    .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_cover_cjk_blocks_only() {
        assert_eq!(slot_for('日'), Some(Slot::Han));
        assert_eq!(slot_for('か'), Some(Slot::Han));
        assert_eq!(slot_for('。'), Some(Slot::Han));
        assert_eq!(slot_for('한'), Some(Slot::Hangul));
        assert_eq!(slot_for('A'), None);
        assert_eq!(slot_for('ب'), None);
        assert_eq!(slot_for('א'), None);
    }

    #[test]
    fn a_loaded_face_is_usable() {
        for slot in [Slot::Han, Slot::Hangul] {
            let Some(l) = loaded(slot) else {
                eprintln!("no system font for {slot:?}; skipped");
                continue;
            };
            eprintln!(
                "{slot:?}: {} face {} ({})",
                l.font.path.display(),
                l.font.index,
                l.font.ps_name
            );
            assert!(l.parsed.glyph_index(probe(slot)).is_some());
            assert!(!l.font.ps_name.is_empty());
        }
    }

    const REGULAR: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");
    const BOLD: &[u8] = include_bytes!("../fonts/NotoSans-Bold.ttf");

    /// A TrueType collection holding `fonts`, built by appending each file and shifting its
    /// table offsets (collections use absolute offsets).
    fn collection(fonts: &[&[u8]]) -> Vec<u8> {
        let mut out = b"ttcf".to_vec();
        out.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        out.extend_from_slice(&u32::try_from(fonts.len()).unwrap().to_be_bytes());
        let header_len = out.len() + 4 * fonts.len();
        out.resize(header_len, 0);
        for (i, font) in fonts.iter().enumerate() {
            while out.len() % 4 != 0 {
                out.push(0);
            }
            let base = u32::try_from(out.len()).unwrap();
            out[12 + 4 * i..16 + 4 * i].copy_from_slice(&base.to_be_bytes());
            let mut copy = font.to_vec();
            let tables = usize::from(u16::from_be_bytes([copy[4], copy[5]]));
            for t in 0..tables {
                let at = 12 + 16 * t + 8;
                let off = u32::from_be_bytes(copy[at..at + 4].try_into().unwrap());
                copy[at..at + 4].copy_from_slice(&(off + base).to_be_bytes());
            }
            out.extend_from_slice(&copy);
        }
        out
    }

    #[test]
    fn every_face_of_a_collection_is_considered() {
        let ttc = collection(&[BOLD, REGULAR]);
        assert_eq!(ttf_parser::fonts_in_collection(&ttc), Some(2));
        let faces = usable_faces(&ttc, Slot::Han, 'A');
        let weights: Vec<u16> = faces.iter().map(|f| f.weight).collect();
        assert_eq!(weights, [700, 400]);
        assert_eq!(faces[1].index, 1);
    }

    #[test]
    fn the_regular_face_is_picked_by_weight_not_by_position() {
        let ttc = collection(&[BOLD, REGULAR]);
        let faces = usable_faces(&ttc, Slot::Han, 'A');
        assert_eq!(pick_regular(&faces).unwrap().index, 1);
        let reversed = collection(&[REGULAR, BOLD]);
        let faces = usable_faces(&reversed, Slot::Han, 'A');
        assert_eq!(pick_regular(&faces).unwrap().index, 0);
    }

    #[test]
    fn faces_without_the_probe_character_are_skipped() {
        let ttc = collection(&[REGULAR, BOLD]);
        assert!(usable_faces(&ttc, Slot::Han, probe(Slot::Han)).is_empty());
        assert!(pick_regular(&[]).is_none());
    }

    #[test]
    fn language_tag_beats_weight_and_position() {
        let jp = FaceInfo {
            index: 0,
            rank: 1,
            weight: 400,
        };
        let sc = FaceInfo {
            index: 2,
            rank: 0,
            weight: 500,
        };
        assert_eq!(pick_regular(&[jp, sc]), Some(sc));
    }

    #[test]
    fn tag_rank_reads_whole_words_of_the_names() {
        let names = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            tag_rank(&names(&["Noto Sans CJK SC", "Regular"]), Slot::Han),
            0
        );
        assert_eq!(tag_rank(&names(&["Noto Sans CJK JP"]), Slot::Han), 1);
        assert_eq!(tag_rank(&names(&["Source Han Sans K"]), Slot::Hangul), 1);
        assert_eq!(tag_rank(&names(&["Noto Sans CJK KR"]), Slot::Hangul), 0);
        assert_eq!(tag_rank(&names(&["Misc Sans"]), Slot::Han), 1);
        assert_eq!(tag_rank(&names(&["PingFang SC"]), Slot::Hangul), 1);
    }
}
