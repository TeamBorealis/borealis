//! App Store validation (error 90717) refuses an app icon with an alpha channel. Every PNG in the iOS
//! AppIcon set must be opaque: an RGB or greyscale colour type and no `tRNS` transparency chunk. The icons
//! are committed files (no build step regenerates them); re-running `tauri icon` writes RGBA PNGs, so
//! flatten them again before committing (this test fails until then).

use std::path::Path;

const ICONS: &str = "gen/apple/Assets.xcassets/AppIcon.appiconset";
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
/// PNG colour types without an alpha channel: greyscale, RGB, palette (opaque unless `tRNS`).
const OPAQUE_COLOUR_TYPES: [u8; 3] = [0, 2, 3];

/// Why the PNG bytes can carry transparency, or `None` when they cannot.
fn alpha_problem(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 33 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return Some("not a PNG".into());
    }
    let colour_type = bytes[25];
    if !OPAQUE_COLOUR_TYPES.contains(&colour_type) {
        return Some(format!("colour type {colour_type} has an alpha channel"));
    }
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind = &bytes[at + 4..at + 8];
        if kind == b"tRNS" {
            return Some("has a tRNS transparency chunk".into());
        }
        if kind == b"IEND" {
            return None;
        }
        at += 12 + len;
    }
    Some("truncated PNG".into())
}

#[test]
fn every_ios_app_icon_is_opaque() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(ICONS);
    let mut checked = 0;
    let mut bad = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("the iOS AppIcon set") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        checked += 1;
        if let Some(why) = alpha_problem(&std::fs::read(&path).unwrap()) {
            bad.push(format!("{}: {why}", path.display()));
        }
    }
    assert!(checked >= 18, "expected the 18 iOS icons, found {checked}");
    assert!(
        bad.is_empty(),
        "App Store refuses icons with alpha (90717):\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_check_spots_alpha_and_transparency() {
    let png = |colour_type: u8, chunk: &[u8; 4]| {
        let mut b = SIGNATURE.to_vec();
        b.extend([0, 0, 0, 13]);
        b.extend(b"IHDR");
        b.extend([0, 0, 0, 1, 0, 0, 0, 1, 8, colour_type, 0, 0, 0]);
        b.extend([0; 4]);
        b.extend([0, 0, 0, 0]);
        b.extend(chunk);
        b.extend([0; 4]);
        b.extend([0, 0, 0, 0]);
        b.extend(b"IEND");
        b.extend([0; 4]);
        b
    };
    assert_eq!(alpha_problem(&png(2, b"IDAT")), None);
    assert!(alpha_problem(&png(6, b"IDAT")).is_some(), "RGBA");
    assert!(alpha_problem(&png(4, b"IDAT")).is_some(), "grey + alpha");
    assert!(
        alpha_problem(&png(3, b"tRNS")).is_some(),
        "palette with transparency"
    );
    assert!(alpha_problem(b"not a png at all, just text......").is_some());
}
