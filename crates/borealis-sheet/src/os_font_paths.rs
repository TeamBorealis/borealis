//! Where to look for system CJK fonts on each operating system.
//!
//! Everything here is a pure function of its inputs (no `cfg`, no environment reads), so the
//! candidate order is unit-tested on every OS; [`crate::os_fonts`] feeds in the real directories.
//! A path that does not exist is simply skipped by the caller, so over-long lists are harmless.

// Each OS uses only its own list; the rest exist so the tests cover them everywhere.
#![allow(dead_code)]

use crate::os_fonts::Slot;
use std::path::{Path, PathBuf};

/// How deep below a font root the Linux scan descends (`/usr/share/fonts/opentype/noto/x.ttc` is
/// three levels down; one spare).
pub(crate) const SCAN_DEPTH: u32 = 4;

/// File extensions a font loader can read.
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

/// Style words that mark a file as a non-regular weight (never a regular-face candidate).
const STYLE_WORDS: [&str; 8] = [
    "bold", "black", "light", "thin", "medium", "heavy", "italic", "oblique",
];

/// Lower-case file-name prefixes of the CJK families searched on Linux, best first.
const LINUX_PREFIXES: [&str; 18] = [
    "notosanscjk-regular",
    "notosanscjk",
    "notosanssc",
    "notosansjp",
    "notosanskr",
    "notosanstc",
    "notoserifcjk",
    "sourcehansans",
    "sourcehanserif",
    "wqy-microhei",
    "wqy-zenhei",
    "droidsansfallback",
    "nanumgothic",
    "undotum",
    "baekmuk",
    "uming",
    "ukai",
    "arplumingcn",
];

fn paths(dir: &str, files: &[&str]) -> Vec<PathBuf> {
    files.iter().map(|f| Path::new(dir).join(f)).collect()
}

/// macOS: system fonts, best Simplified-Chinese-capable face first.
pub(crate) fn macos_candidates(slot: Slot) -> Vec<PathBuf> {
    let sys = "/System/Library/Fonts";
    let supplemental = "/System/Library/Fonts/Supplemental";
    match slot {
        Slot::Han => [
            paths(
                sys,
                &["PingFang.ttc", "Hiragino Sans GB.ttc", "STHeiti Light.ttc"],
            ),
            paths(supplemental, &["Songti.ttc", "Arial Unicode.ttf"]),
            paths("/Library/Fonts", &["Arial Unicode.ttf"]),
        ]
        .concat(),
        Slot::Hangul => [
            paths(sys, &["AppleSDGothicNeo.ttc"]),
            paths(supplemental, &["AppleGothic.ttf", "Arial Unicode.ttf"]),
            paths("/Library/Fonts", &["Arial Unicode.ttf"]),
        ]
        .concat(),
    }
}

/// iOS: guessed locations of the system faces (untested on a device).
pub(crate) fn ios_candidates() -> Vec<PathBuf> {
    [
        paths(
            "/System/Library/Fonts/Core",
            &["PingFang.ttc", "AppleSDGothicNeo.ttc"],
        ),
        paths("/System/Library/Fonts/LanguageSupport", &["PingFang.ttc"]),
        paths(
            "/System/Library/Fonts/CoreAddition",
            &["PingFang.ttc", "AppleSDGothicNeo.ttc"],
        ),
    ]
    .concat()
}

/// Windows: `font_dirs` are the system font folder (`%WINDIR%\Fonts`) then the per-user one
/// (`%LOCALAPPDATA%\Microsoft\Windows\Fonts`). File names are the stock Windows ones: Microsoft
/// YaHei (SC), MS Gothic and Yu Gothic / Meiryo (JP), SimSun and SimHei (SC), Microsoft
/// JhengHei and MingLiU (TC), Malgun Gothic, Gulim and Batang (KR).
pub(crate) fn windows_candidates(slot: Slot, font_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let han = [
        "msyh.ttc",
        "msyh.ttf",
        "msgothic.ttc",
        "YuGothR.ttc",
        "meiryo.ttc",
        "simsun.ttc",
        "simhei.ttf",
        "msjh.ttc",
        "msjh.ttf",
        "mingliu.ttc",
        "malgun.ttf",
    ];
    let hangul = ["malgun.ttf", "gulim.ttc", "batang.ttc", "msyh.ttc"];
    let names: &[&str] = match slot {
        Slot::Han => &han,
        Slot::Hangul => &hangul,
    };
    names
        .iter()
        .flat_map(|name| font_dirs.iter().map(move |dir| dir.join(name)))
        .collect()
}

/// Android: the stock `/system/fonts` CJK files.
pub(crate) fn android_candidates() -> Vec<PathBuf> {
    paths(
        "/system/fonts",
        &[
            "NotoSansCJK-Regular.ttc",
            "NotoSerifCJK-Regular.ttc",
            "NotoSansSC-Regular.otf",
            "NotoSansKR-Regular.otf",
            "DroidSansFallback.ttf",
            "DroidSansFallbackFull.ttf",
        ],
    )
}

/// Directories fontconfig searches by default, plus the XDG and legacy per-user ones.
pub(crate) fn linux_roots(home: Option<&Path>, xdg_data_home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
        PathBuf::from("/run/current-system/sw/share/X11/fonts"),
    ];
    if let Some(data) = xdg_data_home {
        roots.push(data.join("fonts"));
    } else if let Some(home) = home {
        roots.push(home.join(".local/share/fonts"));
    }
    if let Some(home) = home {
        roots.push(home.join(".fonts"));
    }
    roots
}

fn lower_name(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().to_lowercase())
}

fn has_font_extension(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| FONT_EXTENSIONS.contains(&ext))
}

fn prefix_rank(name: &str) -> Option<usize> {
    LINUX_PREFIXES.iter().position(|p| name.starts_with(p))
}

/// Whether `name` (lower case) is a readable font file of a known CJK family.
fn is_cjk_font_file(name: &str) -> bool {
    has_font_extension(name) && prefix_rank(name).is_some()
}

/// Whether a lower-case file name carries a weight or slant word (so it is no regular face).
fn is_styled(name: &str) -> bool {
    STYLE_WORDS.iter().any(|w| name.contains(w))
}

/// Collect CJK-family font files under `dir`, `depth` more directory levels allowed. Unreadable
/// directories are skipped.
pub(crate) fn scan_font_dir(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                scan_font_dir(&path, depth - 1, out);
            }
        } else if lower_name(&path).is_some_and(|n| is_cjk_font_file(&n)) {
            out.push(path);
        }
    }
}

/// Order scanned Linux font files: regular weights only, family rank first (Noto Sans CJK, other
/// Noto/Source Han, WenQuanYi, Droid, Korean families), then path, so the choice does not depend
/// on directory enumeration order.
pub(crate) fn linux_candidates(found: &[PathBuf]) -> Vec<PathBuf> {
    let mut ranked: Vec<(usize, PathBuf)> = found
        .iter()
        .filter_map(|p| {
            let name = lower_name(p)?;
            if !is_cjk_font_file(&name) || is_styled(&name) {
                return None;
            }
            Some((prefix_rank(&name)?, p.clone()))
        })
        .collect();
    ranked.sort();
    ranked.into_iter().map(|(_, p)| p).collect()
}

/// Files that may hold the bold companion of the regular font at `regular`, best first.
///
/// Two rules: a `Regular` in the file name becomes `Bold` (Noto and Source Han naming), and the
/// stock Windows/macOS pairs. Faces inside the same collection are handled by the caller.
pub(crate) fn bold_siblings(regular: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(name)) = (regular.parent(), regular.file_name()) else {
        return Vec::new();
    };
    let name = name.to_string_lossy();
    let lower = name.to_lowercase();
    let mut out = Vec::new();
    if let Some(at) = lower.find("regular") {
        out.push(format!(
            "{}Bold{}",
            &name[..at],
            &name[at + "regular".len()..]
        ));
    }
    let paired = match lower.as_str() {
        "msyh.ttc" => Some("msyhbd.ttc"),
        "msyh.ttf" => Some("msyhbd.ttf"),
        "msjh.ttc" => Some("msjhbd.ttc"),
        "msjh.ttf" => Some("msjhbd.ttf"),
        "yugothr.ttc" => Some("YuGothB.ttc"),
        "meiryo.ttc" => Some("meiryob.ttc"),
        "malgun.ttf" => Some("malgunbd.ttf"),
        "stheiti light.ttc" => Some("STHeiti Medium.ttc"),
        _ => None,
    };
    out.extend(paired.map(str::to_owned));
    out.into_iter().map(|n| dir.join(n)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect()
    }

    #[test]
    fn windows_han_order_starts_with_yahei_then_japanese_then_simsun() {
        let dirs = [PathBuf::from("Fonts")];
        let list = names(&windows_candidates(Slot::Han, &dirs));
        let pos = |n: &str| list.iter().position(|x| x == n).unwrap();
        assert_eq!(list[0], "msyh.ttc");
        assert!(pos("msyh.ttc") < pos("msgothic.ttc"));
        assert!(pos("msgothic.ttc") < pos("simsun.ttc"));
        assert!(list.iter().any(|n| n == "malgun.ttf"));
    }

    #[test]
    fn windows_hangul_prefers_malgun_and_checks_system_before_user_dir() {
        let dirs = [PathBuf::from("sys"), PathBuf::from("user")];
        let list = windows_candidates(Slot::Hangul, &dirs);
        assert_eq!(list[0], Path::new("sys").join("malgun.ttf"));
        assert_eq!(list[1], Path::new("user").join("malgun.ttf"));
        assert!(names(&list).contains(&"gulim.ttc".to_owned()));
    }

    #[test]
    fn macos_lists_cover_both_slots_with_absolute_system_paths() {
        let han = macos_candidates(Slot::Han);
        assert_eq!(han[0], Path::new("/System/Library/Fonts/PingFang.ttc"));
        let hangul = macos_candidates(Slot::Hangul);
        assert_eq!(names(&hangul)[0], "AppleSDGothicNeo.ttc");
        assert!(han.iter().chain(&hangul).all(|p| p.starts_with("/")));
    }

    #[test]
    fn android_and_ios_lists_are_not_empty() {
        assert_eq!(names(&android_candidates())[0], "NotoSansCJK-Regular.ttc");
        assert!(!ios_candidates().is_empty());
    }

    #[test]
    fn linux_roots_include_fontconfig_defaults_and_per_user_dirs() {
        let roots = linux_roots(Some(Path::new("/home/u")), None);
        assert!(roots.contains(&PathBuf::from("/usr/share/fonts")));
        assert!(roots.contains(&PathBuf::from("/home/u/.local/share/fonts")));
        assert!(roots.contains(&PathBuf::from("/home/u/.fonts")));
        let xdg = linux_roots(Some(Path::new("/home/u")), Some(Path::new("/data")));
        assert!(xdg.contains(&PathBuf::from("/data/fonts")));
        assert!(!xdg.contains(&PathBuf::from("/home/u/.local/share/fonts")));
        assert_eq!(linux_roots(None, None).len(), 3);
    }

    #[test]
    fn linux_order_ranks_noto_regular_first_and_drops_styled_or_unrelated_files() {
        let found: Vec<PathBuf> = [
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
            "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-DemiLight.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc.md",
            "/usr/share/fonts/opentype/source-han/SourceHanSansSC-Regular.otf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        let list = names(&linux_candidates(&found));
        assert_eq!(
            list,
            [
                "NotoSansCJK-Regular.ttc",
                "NotoSerifCJK-Regular.ttc",
                "SourceHanSansSC-Regular.otf",
                "wqy-microhei.ttc",
                "DroidSansFallbackFull.ttf",
            ]
        );
    }

    #[test]
    fn linux_order_is_independent_of_input_order() {
        let a = PathBuf::from("/usr/share/fonts/a/NotoSansCJK-Regular.ttc");
        let b = PathBuf::from("/usr/share/fonts/b/NotoSansCJK-Regular.ttc");
        assert_eq!(
            linux_candidates(&[b.clone(), a.clone()]),
            linux_candidates(&[a, b])
        );
    }

    #[test]
    fn scan_finds_nested_cjk_fonts_and_respects_the_depth_limit() {
        let root = std::env::temp_dir().join(format!("borealis-fontscan-{}", std::process::id()));
        let deep = root.join("a/b/c/d/e");
        let mid = root.join("opentype/noto");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(&mid).unwrap();
        std::fs::write(deep.join("NotoSansCJK-Regular.ttc"), b"x").unwrap();
        std::fs::write(mid.join("NotoSansCJK-Regular.ttc"), b"x").unwrap();
        std::fs::write(mid.join("README.txt"), b"x").unwrap();
        let mut out = Vec::new();
        scan_font_dir(&root, SCAN_DEPTH, &mut out);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].ends_with(Path::new("opentype/noto/NotoSansCJK-Regular.ttc")));
    }

    #[test]
    fn a_missing_scan_root_yields_nothing_and_does_not_panic() {
        let mut out = Vec::new();
        scan_font_dir(Path::new("/definitely/not/here"), SCAN_DEPTH, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn bold_siblings_follow_noto_and_windows_naming() {
        let noto = bold_siblings(Path::new("/f/NotoSansCJK-Regular.ttc"));
        assert_eq!(noto, [PathBuf::from("/f/NotoSansCJK-Bold.ttc")]);
        let han = bold_siblings(Path::new("/f/SourceHanSansSC-Regular.otf"));
        assert_eq!(han, [PathBuf::from("/f/SourceHanSansSC-Bold.otf")]);
        let yahei = bold_siblings(Path::new("/w/MSYH.TTC"));
        assert_eq!(yahei, [PathBuf::from("/w/msyhbd.ttc")]);
        assert_eq!(
            names(&bold_siblings(Path::new("/w/malgun.ttf"))),
            ["malgunbd.ttf"]
        );
        assert!(bold_siblings(Path::new("/f/simsun.ttc")).is_empty());
        assert!(bold_siblings(Path::new("/")).is_empty());
    }
}
