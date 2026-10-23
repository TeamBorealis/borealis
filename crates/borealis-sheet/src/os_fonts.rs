//! CJK fallback faces loaded from the operating system at render time.
//!
//! Full CJK fonts are tens of megabytes, so none is compiled in. The first time a sheet needs a
//! Han/kana or Hangul character that no embedded face covers, one installed font per slot is
//! loaded (first usable candidate in a fixed priority order, so the choice is stable for a given
//! machine) and later subset like the embedded faces. Without a usable font the characters keep
//! drawing `.notdef`, as before.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
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
    pub data: Vec<u8>,
    /// Face index inside a TrueType collection (0 for a plain font file).
    pub index: u32,
    /// PostScript name, reduced to characters valid in a PDF name.
    pub ps_name: String,
    pub path: PathBuf,
}

/// A loaded font and its parsed face.
pub(crate) struct Loaded {
    pub font: &'static OsFont,
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
    let han = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];
    let hangul = [
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];
    let list: &[&str] = match slot {
        Slot::Han => &han,
        Slot::Hangul => &hangul,
    };
    list.iter().map(PathBuf::from).collect()
}

#[cfg(target_os = "ios")]
fn candidates(_slot: Slot) -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/Core/PingFang.ttc",
        "/System/Library/Fonts/LanguageSupport/PingFang.ttc",
        "/System/Library/Fonts/CoreAddition/PingFang.ttc",
        "/System/Library/Fonts/Core/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/CoreAddition/AppleSDGothicNeo.ttc",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(target_os = "windows")]
fn candidates(slot: Slot) -> Vec<PathBuf> {
    let root =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let fonts = root.join("Fonts");
    let han = [
        "msyh.ttc",
        "msyh.ttf",
        "msgothic.ttc",
        "simsun.ttc",
        "YuGothR.ttc",
        "malgun.ttf",
    ];
    let hangul = ["malgun.ttf", "gulim.ttc", "batang.ttc", "msyh.ttc"];
    let list: &[&str] = match slot {
        Slot::Han => &han,
        Slot::Hangul => &hangul,
    };
    list.iter().map(|f| fonts.join(f)).collect()
}

#[cfg(target_os = "android")]
fn candidates(_slot: Slot) -> Vec<PathBuf> {
    [
        "/system/fonts/NotoSansCJK-Regular.ttc",
        "/system/fonts/NotoSerifCJK-Regular.ttc",
        "/system/fonts/NotoSansSC-Regular.otf",
        "/system/fonts/NotoSansKR-Regular.otf",
        "/system/fonts/DroidSansFallback.ttf",
        "/system/fonts/DroidSansFallbackFull.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn candidates(_slot: Slot) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/fonts"));
    }
    let mut found = Vec::new();
    for root in &roots {
        scan(root, LINUX_SCAN_DEPTH, &mut found);
    }
    // Priority by file-name prefix, then path, so the choice does not depend on directory order.
    found.sort_by_key(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
        let rank = name.as_deref().map_or(usize::MAX, |n| {
            LINUX_PREFIXES
                .iter()
                .position(|pre| n.starts_with(pre))
                .unwrap_or(usize::MAX)
        });
        (rank, p.clone())
    });
    found
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
const LINUX_PREFIXES: [&str; 6] = [
    "NotoSansCJK-Regular",
    "NotoSansCJK",
    "NotoSerifCJK",
    "SourceHanSans",
    "wqy-microhei",
    "wqy-zenhei",
];

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
const LINUX_SCAN_DEPTH: u32 = 4;

/// Font files under `dir` whose names match a known CJK family (depth-limited).
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn scan(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                scan(&path, depth - 1, out);
            }
        } else if path.file_name().is_some_and(|n| {
            let n = n.to_string_lossy();
            LINUX_PREFIXES.iter().any(|pre| n.starts_with(pre))
        }) {
            out.push(path);
        }
    }
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

/// Whether a parsed face can stand in for `slot`: outlines the PDF writer embeds, the probe
/// character, and a licence that allows embedding a subset.
fn usable(parsed: &Parsed<'_>, slot: Slot) -> bool {
    let tables = parsed.tables();
    let outlines = tables.glyf.is_some() || tables.cff.is_some();
    let licensed = parsed.permissions() != Some(ttf_parser::Permissions::Restricted)
        && parsed.is_subsetting_allowed();
    outlines && licensed && parsed.glyph_index(probe(slot)).is_some()
}

/// Read `path` and keep it when its first face is usable for `slot`.
fn open(path: &Path, slot: Slot) -> Option<OsFont> {
    let data = std::fs::read(path).ok()?;
    let index = 0;
    let parsed = Parsed::parse(&data, index).ok()?;
    if !usable(&parsed, slot) {
        return None;
    }
    let ps_name = clean_ps_name(&parsed, path);
    Some(OsFont {
        data,
        index,
        ps_name,
        path: path.to_path_buf(),
    })
}

fn find(slot: Slot) -> Option<OsFont> {
    let found = candidates(slot).iter().find_map(|p| open(p, slot));
    match &found {
        Some(f) => tracing::debug!("sheet: system CJK font for {slot:?}: {}", f.path.display()),
        None => tracing::warn!(
            "sheet: no system font for {slot:?} text; those characters render as missing glyphs"
        ),
    }
    found
}

fn file(slot: Slot) -> Option<&'static OsFont> {
    static HAN: OnceLock<Option<OsFont>> = OnceLock::new();
    static HANGUL: OnceLock<Option<OsFont>> = OnceLock::new();
    let cell = match slot {
        Slot::Han => &HAN,
        Slot::Hangul => &HANGUL,
    };
    cell.get_or_init(|| find(slot)).as_ref()
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
        let font = file(slot)?;
        // `open` already parsed this data once, so this cannot fail in practice.
        let parsed = Parsed::parse(&font.data, font.index).ok()?;
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
            assert!(l.parsed.glyph_index(probe(slot)).is_some());
            assert!(!l.font.ps_name.is_empty());
        }
    }
}
