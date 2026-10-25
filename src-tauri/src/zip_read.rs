//! A small, safe `.zip` reader for content imports (stored and deflate entries only).
//!
//! Only entries the importer cares about are decompressed. Names are checked against path
//! traversal, and entry count and decompressed size are capped so an archive bomb cannot
//! exhaust memory. Zip64 and encrypted archives are refused with a clear message.

use crate::error::{ApiError, ApiResult};

/// Most entries an archive may hold.
pub const MAX_ENTRIES: usize = 5_000;
/// Most decompressed bytes accepted from one archive.
pub const MAX_TOTAL_BYTES: usize = 128 * 1024 * 1024;

const EOCD_SIG: u32 = 0x0605_4b50;
const CEN_SIG: u32 = 0x0201_4b50;
const LOC_SIG: u32 = 0x0403_4b50;
const EOCD_MIN: usize = 22;
const MAX_COMMENT: usize = 65_535;
const CEN_FIXED: usize = 46;
const LOC_FIXED: usize = 30;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn bad() -> ApiError {
    ApiError::invalid("That .zip file is damaged or uses an unsupported format.")
}

/// True for a relative, traversal-free entry name that names the same file on every OS: no
/// segment is a Windows device name (`nul.json` would write to the device), and none ends in a
/// dot or space (Windows drops those, so `class./x.json` would land in `class/`).
pub(crate) fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.contains('\0')
        && name.split('/').all(|p| {
            p == "."
                || (p != ".."
                    && !p.ends_with(['.', ' '])
                    && !borealis_bundle::is_windows_reserved(p))
        })
}

/// Caps and name policy for one archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZipLimits {
    /// Most entries the archive may list (directories included).
    pub max_entries: usize,
    /// Most decompressed bytes taken from the archive in total.
    pub max_total_bytes: usize,
    /// Most decompressed bytes of one taken entry.
    pub max_entry_bytes: usize,
    /// Refuse the whole archive when any entry name is unsafe (otherwise such entries are skipped).
    pub refuse_unsafe: bool,
}

/// The limits [`read_zip`] applies (content packs).
pub const PACK_LIMITS: ZipLimits = ZipLimits {
    max_entries: MAX_ENTRIES,
    max_total_bytes: MAX_TOTAL_BYTES,
    max_entry_bytes: MAX_TOTAL_BYTES,
    refuse_unsafe: false,
};

/// One central directory record.
struct CenEntry {
    name: String,
    flags: u16,
    method: u16,
    csize: usize,
    size: usize,
    local: usize,
}

impl CenEntry {
    fn is_dir(&self) -> bool {
        self.name.as_bytes().last() == Some(&b'/')
    }
}

fn central_directory(data: &[u8], max_entries: usize) -> ApiResult<Vec<CenEntry>> {
    let start = data.len().saturating_sub(EOCD_MIN + MAX_COMMENT);
    let eocd = (start..=data.len().saturating_sub(EOCD_MIN))
        .rev()
        .find(|&i| u32_at(data, i) == Some(EOCD_SIG))
        .ok_or_else(bad)?;
    let count = usize::from(u16_at(data, eocd + 10).ok_or_else(bad)?);
    let mut at = u32_at(data, eocd + 16).ok_or_else(bad)? as usize;
    if count == usize::from(u16::MAX) || at == u32::MAX as usize {
        return Err(bad());
    }
    if count > max_entries {
        return Err(ApiError::invalid("That .zip file holds too many files."));
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(data, at) != Some(CEN_SIG) {
            return Err(bad());
        }
        let nlen = usize::from(u16_at(data, at + 28).ok_or_else(bad)?);
        let xlen = usize::from(u16_at(data, at + 30).ok_or_else(bad)?);
        let clen = usize::from(u16_at(data, at + 32).ok_or_else(bad)?);
        let name_bytes = data
            .get(at + CEN_FIXED..at + CEN_FIXED + nlen)
            .ok_or_else(bad)?;
        out.push(CenEntry {
            name: String::from_utf8_lossy(name_bytes).into_owned(),
            flags: u16_at(data, at + 8).ok_or_else(bad)?,
            method: u16_at(data, at + 10).ok_or_else(bad)?,
            csize: u32_at(data, at + 20).ok_or_else(bad)? as usize,
            size: u32_at(data, at + 24).ok_or_else(bad)? as usize,
            local: u32_at(data, at + 42).ok_or_else(bad)? as usize,
        });
        at += CEN_FIXED + nlen + xlen + clen;
    }
    Ok(out)
}

fn unsafe_entry() -> ApiError {
    ApiError::invalid("That .zip file contains a file path that points outside the archive.")
}

/// The names of every file (not directory) in the archive, in archive order.
///
/// # Errors
/// `invalid_argument` for a damaged or oversized archive, or (with `refuse_unsafe`) an unsafe name.
pub fn entry_names(data: &[u8], limits: &ZipLimits) -> ApiResult<Vec<String>> {
    let entries = central_directory(data, limits.max_entries)?;
    if limits.refuse_unsafe && entries.iter().any(|e| !safe_name(&e.name)) {
        return Err(unsafe_entry());
    }
    Ok(entries
        .into_iter()
        .filter(|e| !e.is_dir() && safe_name(&e.name))
        .map(|e| e.name)
        .collect())
}

fn inflate(data: &[u8], e: &CenEntry) -> ApiResult<Vec<u8>> {
    if u32_at(data, e.local) != Some(LOC_SIG) {
        return Err(bad());
    }
    let lname = usize::from(u16_at(data, e.local + 26).ok_or_else(bad)?);
    let lextra = usize::from(u16_at(data, e.local + 28).ok_or_else(bad)?);
    let body_at = e.local + LOC_FIXED + lname + lextra;
    let body = data.get(body_at..body_at + e.csize).ok_or_else(bad)?;
    match e.method {
        METHOD_STORED if e.csize == e.size => Ok(body.to_vec()),
        METHOD_DEFLATE => miniz_oxide::inflate::decompress_to_vec_with_limit(body, e.size + 1)
            .ok()
            .filter(|b| b.len() == e.size)
            .ok_or_else(bad),
        _ => Err(bad()),
    }
}

/// Decompress every file entry whose (unchanged) name satisfies `wanted` and hand it to `take`,
/// in archive order, within `limits`.
///
/// # Errors
/// `invalid_argument` for a damaged, encrypted, Zip64 or oversized archive or (with
/// `refuse_unsafe`) an unsafe name; any error `take` returns.
pub fn for_each_entry(
    data: &[u8],
    limits: &ZipLimits,
    wanted: impl Fn(&str) -> bool,
    mut take: impl FnMut(&str, Vec<u8>) -> ApiResult<()>,
) -> ApiResult<()> {
    let entries = central_directory(data, limits.max_entries)?;
    if limits.refuse_unsafe && entries.iter().any(|e| !safe_name(&e.name)) {
        return Err(unsafe_entry());
    }
    let mut total = 0usize;
    for e in &entries {
        if e.is_dir() || !wanted(&e.name) || !safe_name(&e.name) {
            continue;
        }
        if e.flags & 1 != 0 {
            return Err(ApiError::invalid("Encrypted .zip files are not supported."));
        }
        total = total.saturating_add(e.size);
        if total > limits.max_total_bytes || e.size > limits.max_entry_bytes {
            return Err(ApiError::invalid("That .zip file is too large to import."));
        }
        take(&e.name, inflate(data, e)?)?;
    }
    Ok(())
}

/// Read the entries whose lower-cased name satisfies `wanted`; returns `(name, bytes)`.
/// Unsafe names are skipped.
///
/// # Errors
/// `invalid_argument` for a damaged, encrypted, Zip64 or oversized archive.
pub fn read_zip(data: &[u8], wanted: impl Fn(&str) -> bool) -> ApiResult<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for_each_entry(
        data,
        &PACK_LIMITS,
        |name| wanted(&name.to_lowercase()),
        |name, bytes| {
            out.push((name.to_string(), bytes));
            Ok(())
        },
    )?;
    Ok(out)
}

/// Hand-built archives for tests (stored or deflate entries, no CRC check is made by the reader).
#[cfg(test)]
pub(crate) mod test_zip {
    use super::{CEN_SIG, EOCD_SIG, LOC_SIG, METHOD_DEFLATE, METHOD_STORED};

    /// An archive of `(name, body)` entries; `deflate` compresses every body.
    pub(crate) fn build(entries: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
        let method = if deflate {
            METHOD_DEFLATE
        } else {
            METHOD_STORED
        };
        let mut z = Vec::new();
        let mut cen = Vec::new();
        for (name, body) in entries {
            let packed = if deflate {
                miniz_oxide::deflate::compress_to_vec(body, 6)
            } else {
                body.to_vec()
            };
            let local = z.len() as u32;
            let sizes = |v: &mut Vec<u8>| {
                v.extend(0u32.to_le_bytes());
                v.extend((packed.len() as u32).to_le_bytes());
                v.extend((body.len() as u32).to_le_bytes());
                v.extend((name.len() as u16).to_le_bytes());
            };
            z.extend(LOC_SIG.to_le_bytes());
            z.extend([20, 0, 0, 0]);
            z.extend(method.to_le_bytes());
            z.extend([0, 0, 0, 0]);
            sizes(&mut z);
            z.extend([0, 0]);
            z.extend(name.as_bytes());
            z.extend(&packed);
            cen.extend(CEN_SIG.to_le_bytes());
            cen.extend([20, 0, 20, 0, 0, 0]);
            cen.extend(method.to_le_bytes());
            cen.extend([0, 0, 0, 0]);
            sizes(&mut cen);
            cen.extend([0; 12]);
            cen.extend(local.to_le_bytes());
            cen.extend(name.as_bytes());
        }
        let at = z.len() as u32;
        let n = entries.len() as u16;
        z.extend(&cen);
        z.extend(EOCD_SIG.to_le_bytes());
        z.extend([0, 0, 0, 0]);
        z.extend(n.to_le_bytes());
        z.extend(n.to_le_bytes());
        z.extend((cen.len() as u32).to_le_bytes());
        z.extend(at.to_le_bytes());
        z.extend([0, 0]);
        z
    }
}

#[cfg(test)]
mod tests {
    use super::test_zip::build;
    use super::*;

    fn stored_zip(name: &str, body: &[u8]) -> Vec<u8> {
        build(&[(name, body)], false)
    }

    #[test]
    fn reads_a_stored_entry() {
        let z = stored_zip("dir/a.xml", b"<elements/>");
        let got = read_zip(&z, |n| n.ends_with(".xml")).unwrap();
        assert_eq!(
            got,
            vec![("dir/a.xml".to_string(), b"<elements/>".to_vec())]
        );
    }

    #[test]
    fn reads_deflated_entries_in_order() {
        let z = build(&[("b.xml", b"<b/>"), ("A.XML", b"<a/>")], true);
        let got = read_zip(&z, |n| n.ends_with(".xml")).unwrap();
        assert_eq!(
            got,
            vec![
                ("b.xml".to_string(), b"<b/>".to_vec()),
                ("A.XML".to_string(), b"<a/>".to_vec())
            ]
        );
    }

    #[test]
    fn skips_unsafe_and_unwanted_names() {
        let z = stored_zip("../evil.xml", b"x");
        assert!(read_zip(&z, |_| true).unwrap().is_empty());
        let z = stored_zip("a.txt", b"x");
        assert!(read_zip(&z, |n| n.ends_with(".xml")).unwrap().is_empty());
    }

    #[test]
    fn strict_limits_refuse_unsafe_names_anywhere_in_the_archive() {
        let strict = ZipLimits {
            refuse_unsafe: true,
            ..PACK_LIMITS
        };
        for evil in [
            "../evil.json",
            "/abs.json",
            "a/../../b.json",
            "c:/x.json",
            "a\\b.json",
        ] {
            let z = build(&[("ok.json", b"{}"), (evil, b"{}")], false);
            let err = for_each_entry(&z, &strict, |_| true, |_, _| Ok(())).unwrap_err();
            assert!(err.message.contains("outside"), "{evil}: {}", err.message);
            assert!(entry_names(&z, &strict).is_err(), "{evil}");
        }
    }

    #[test]
    fn names_windows_would_resolve_elsewhere_are_unsafe_on_every_host() {
        for evil in [
            "nul.json",
            "class/CON.json",
            "aux",
            "bestiary/com1.json",
            "LPT9.txt",
            "class./x.json",
            "class /x.json",
            "x.json.",
        ] {
            assert!(!safe_name(evil), "{evil}");
        }
        for fine in [
            "class/class-x.json",
            "./races.json",
            "console.json",
            "nullable.json",
        ] {
            assert!(safe_name(fine), "{fine}");
        }
    }

    #[test]
    fn caps_entry_count_total_and_entry_size() {
        let z = build(
            &[("a.json", b"0123456789"), ("b.json", b"0123456789")],
            true,
        );
        let few = ZipLimits {
            max_entries: 1,
            ..PACK_LIMITS
        };
        assert!(entry_names(&z, &few).is_err());
        let small_total = ZipLimits {
            max_total_bytes: 15,
            ..PACK_LIMITS
        };
        assert!(for_each_entry(&z, &small_total, |_| true, |_, _| Ok(())).is_err());
        let small_entry = ZipLimits {
            max_entry_bytes: 9,
            ..PACK_LIMITS
        };
        assert!(for_each_entry(&z, &small_entry, |_| true, |_, _| Ok(())).is_err());
        let mut seen = 0;
        for_each_entry(
            &z,
            &PACK_LIMITS,
            |n| n == "b.json",
            |_, b| {
                seen += b.len();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(seen, 10);
    }

    #[test]
    fn a_body_larger_than_its_declared_size_is_refused() {
        let mut z = build(&[("a.json", b"0123456789")], true);
        // Shrink the declared uncompressed size in the central directory (offset 24 of the record).
        let cen = z.len() - 22 - (46 + 6);
        z[cen + 24] = 4;
        assert!(for_each_entry(&z, &PACK_LIMITS, |_| true, |_, _| Ok(())).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(read_zip(b"not a zip at all", |_| true).is_err());
    }
}
