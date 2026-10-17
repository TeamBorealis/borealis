//! Byte-level helpers: a stored (uncompressed) zip writer for the folder-vs-zip check and a
//! digest of an installed pack directory for the byte-identical rerun check.

use sha2::{Digest, Sha256};
use std::path::Path;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in bytes {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn u16le(out: &mut Vec<u8>, v: usize) {
    out.extend_from_slice(&u16::try_from(v).expect("zip u16").to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, v: usize) {
    out.extend_from_slice(&u32::try_from(v).expect("zip u32").to_le_bytes());
}

/// A zip (method 0, stored) of `(name, bytes)` entries, in the given order.
pub fn zip_stored(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len();
        let crc = crc32(data) as usize;
        // local header
        u32le(&mut out, 0x0403_4b50);
        for v in [20, 0, 0, 0, 0x21] {
            u16le(&mut out, v);
        }
        u32le(&mut out, crc);
        u32le(&mut out, data.len());
        u32le(&mut out, data.len());
        u16le(&mut out, name.len());
        u16le(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // central directory record
        u32le(&mut central, 0x0201_4b50);
        for v in [20, 20, 0, 0, 0, 0x21] {
            u16le(&mut central, v);
        }
        u32le(&mut central, crc);
        u32le(&mut central, data.len());
        u32le(&mut central, data.len());
        u16le(&mut central, name.len());
        for v in [0, 0, 0, 0] {
            u16le(&mut central, v);
        }
        u32le(&mut central, 0);
        u32le(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len();
    let size = central.len();
    out.extend_from_slice(&central);
    u32le(&mut out, 0x0605_4b50);
    for v in [0, 0, entries.len(), entries.len()] {
        u16le(&mut out, v);
    }
    u32le(&mut out, size);
    u32le(&mut out, start);
    u16le(&mut out, 0);
    out
}

/// Entries for every file under `dir` (relative names with `/`, prefixed), sorted.
pub fn tree_entries(
    dir: &Path,
    prefix: &str,
    keep: &dyn Fn(&str) -> bool,
) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    collect(dir, dir, prefix, keep, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect(
    root: &Path,
    dir: &Path,
    prefix: &str,
    keep: &dyn Fn(&str) -> bool,
    out: &mut Vec<(String, Vec<u8>)>,
) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(root, &p, prefix, keep, out);
            continue;
        }
        let rel = p
            .strip_prefix(root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if keep(&rel) {
            if let Ok(bytes) = std::fs::read(&p) {
                out.push((format!("{prefix}{rel}"), bytes));
            }
        }
    }
}

/// SHA-256 over the sorted relative paths and bytes of every file under `dir`.
pub fn digest_dir(dir: &Path) -> String {
    let mut h = Sha256::new();
    for (name, bytes) in tree_entries(dir, "", &|_| true) {
        h.update(name.as_bytes());
        h.update([0]);
        h.update(&bytes);
        h.update([0]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_of_a_known_string() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}
