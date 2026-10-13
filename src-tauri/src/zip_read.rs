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

/// True for a relative, traversal-free entry name.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && name.split('/').all(|p| p != "..")
}

/// Read the entries whose lower-cased name satisfies `wanted`; returns `(name, bytes)`.
///
/// # Errors
/// `invalid_argument` for a damaged, encrypted, Zip64 or oversized archive.
pub fn read_zip(data: &[u8], wanted: impl Fn(&str) -> bool) -> ApiResult<Vec<(String, Vec<u8>)>> {
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
    if count > MAX_ENTRIES {
        return Err(ApiError::invalid("That .zip file holds too many files."));
    }
    let mut out = Vec::new();
    let mut total = 0usize;
    for _ in 0..count {
        if u32_at(data, at) != Some(CEN_SIG) {
            return Err(bad());
        }
        let flags = u16_at(data, at + 8).ok_or_else(bad)?;
        let method = u16_at(data, at + 10).ok_or_else(bad)?;
        let csize = u32_at(data, at + 20).ok_or_else(bad)? as usize;
        let usize_ = u32_at(data, at + 24).ok_or_else(bad)? as usize;
        let nlen = usize::from(u16_at(data, at + 28).ok_or_else(bad)?);
        let xlen = usize::from(u16_at(data, at + 30).ok_or_else(bad)?);
        let clen = usize::from(u16_at(data, at + 32).ok_or_else(bad)?);
        let local = u32_at(data, at + 42).ok_or_else(bad)? as usize;
        let name_bytes = data
            .get(at + CEN_FIXED..at + CEN_FIXED + nlen)
            .ok_or_else(bad)?;
        at += CEN_FIXED + nlen + xlen + clen;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        if name.ends_with('/') || !wanted(&name.to_lowercase()) {
            continue;
        }
        if !safe_name(&name) {
            continue;
        }
        if flags & 1 != 0 {
            return Err(ApiError::invalid("Encrypted .zip files are not supported."));
        }
        total = total.saturating_add(usize_);
        if total > MAX_TOTAL_BYTES {
            return Err(ApiError::invalid("That .zip file is too large to import."));
        }
        if u32_at(data, local) != Some(LOC_SIG) {
            return Err(bad());
        }
        let lname = usize::from(u16_at(data, local + 26).ok_or_else(bad)?);
        let lextra = usize::from(u16_at(data, local + 28).ok_or_else(bad)?);
        let body_at = local + LOC_FIXED + lname + lextra;
        let body = data.get(body_at..body_at + csize).ok_or_else(bad)?;
        let bytes = match method {
            METHOD_STORED => body.to_vec(),
            METHOD_DEFLATE => miniz_oxide::inflate::decompress_to_vec_with_limit(body, usize_ + 1)
                .map_err(|_| bad())?,
            _ => return Err(bad()),
        };
        out.push((name, bytes));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored (uncompressed) single-entry archive built by hand.
    fn stored_zip(name: &str, body: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        let crc = 0u32;
        z.extend(LOC_SIG.to_le_bytes());
        z.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend(crc.to_le_bytes());
        z.extend((body.len() as u32).to_le_bytes());
        z.extend((body.len() as u32).to_le_bytes());
        z.extend((name.len() as u16).to_le_bytes());
        z.extend([0, 0]);
        z.extend(name.as_bytes());
        z.extend(body);
        let cen = z.len();
        z.extend(CEN_SIG.to_le_bytes());
        z.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend(crc.to_le_bytes());
        z.extend((body.len() as u32).to_le_bytes());
        z.extend((body.len() as u32).to_le_bytes());
        z.extend((name.len() as u16).to_le_bytes());
        z.extend([0; 12]);
        z.extend(0u32.to_le_bytes());
        z.extend(name.as_bytes());
        let cen_len = z.len() - cen;
        z.extend(EOCD_SIG.to_le_bytes());
        z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend((cen_len as u32).to_le_bytes());
        z.extend((cen as u32).to_le_bytes());
        z.extend([0, 0]);
        z
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
    fn skips_unsafe_and_unwanted_names() {
        let z = stored_zip("../evil.xml", b"x");
        assert!(read_zip(&z, |_| true).unwrap().is_empty());
        let z = stored_zip("a.txt", b"x");
        assert!(read_zip(&z, |n| n.ends_with(".xml")).unwrap().is_empty());
    }

    #[test]
    fn rejects_garbage() {
        assert!(read_zip(b"not a zip at all", |_| true).is_err());
    }
}
