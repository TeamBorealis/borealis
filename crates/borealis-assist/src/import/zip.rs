//! A minimal ZIP reader for office documents (`.docx`, `.odt`): stored and deflated entries,
//! no ZIP64, no encryption. Every entry is size-capped (zip-bomb guard).

use std::io::Read;

use flate2::read::DeflateDecoder;

use crate::error::AssistError;

/// Largest uncompressed entry read (a document's XML is far smaller).
pub const MAX_ENTRY_BYTES: usize = 64 * 1024 * 1024;

const EOCD_SIG: u32 = 0x0605_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
/// End-of-central-directory record without its comment.
const EOCD_LEN: usize = 22;
/// Longest ZIP comment, so the furthest the EOCD record can sit from the end.
const MAX_COMMENT: usize = 0xFFFF;
const CENTRAL_LEN: usize = 46;
const LOCAL_LEN: usize = 30;

fn bad(what: &str) -> AssistError {
    AssistError::Extraction(format!("the document could not be read ({what})"))
}

fn u16_at(b: &[u8], at: usize) -> Result<usize, AssistError> {
    b.get(at..at + 2)
        .map(|s| usize::from(u16::from_le_bytes([s[0], s[1]])))
        .ok_or_else(|| bad("truncated archive"))
}

fn u32_at(b: &[u8], at: usize) -> Result<usize, AssistError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
        .ok_or_else(|| bad("truncated archive"))
}

/// One entry of the central directory.
struct Entry {
    name: String,
    method: usize,
    compressed: usize,
    size: usize,
    local: usize,
}

fn eocd(bytes: &[u8]) -> Result<usize, AssistError> {
    if bytes.len() < EOCD_LEN {
        return Err(bad("not a ZIP archive"));
    }
    let lowest = bytes.len().saturating_sub(EOCD_LEN + MAX_COMMENT);
    (lowest..=bytes.len() - EOCD_LEN)
        .rev()
        .find(|&i| u32_at(bytes, i).ok() == Some(EOCD_SIG as usize))
        .ok_or_else(|| bad("not a ZIP archive"))
}

fn entries(bytes: &[u8]) -> Result<Vec<Entry>, AssistError> {
    let end = eocd(bytes)?;
    let count = u16_at(bytes, end + 10)?;
    let mut at = u32_at(bytes, end + 16)?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(bytes, at)? != CENTRAL_SIG as usize {
            return Err(bad("damaged directory"));
        }
        let name_len = u16_at(bytes, at + 28)?;
        let name = bytes
            .get(at + CENTRAL_LEN..at + CENTRAL_LEN + name_len)
            .ok_or_else(|| bad("truncated archive"))?;
        out.push(Entry {
            name: String::from_utf8_lossy(name).into_owned(),
            method: u16_at(bytes, at + 10)?,
            compressed: u32_at(bytes, at + 20)?,
            size: u32_at(bytes, at + 24)?,
            local: u32_at(bytes, at + 42)?,
        });
        at += CENTRAL_LEN + name_len + u16_at(bytes, at + 30)? + u16_at(bytes, at + 32)?;
    }
    Ok(out)
}

/// The uncompressed bytes of `name`, or `None` when the archive has no such entry.
///
/// # Errors
/// [`AssistError::Extraction`] for a damaged archive, an unsupported compression method or an
/// entry larger than [`MAX_ENTRY_BYTES`].
pub fn read_entry(bytes: &[u8], name: &str) -> Result<Option<Vec<u8>>, AssistError> {
    let Some(e) = entries(bytes)?.into_iter().find(|e| e.name == name) else {
        return Ok(None);
    };
    if e.size > MAX_ENTRY_BYTES {
        return Err(bad("an entry is too large"));
    }
    if u32_at(bytes, e.local)? != LOCAL_SIG as usize {
        return Err(bad("damaged entry"));
    }
    let start = e.local + LOCAL_LEN + u16_at(bytes, e.local + 26)? + u16_at(bytes, e.local + 28)?;
    let data = bytes
        .get(start..start + e.compressed)
        .ok_or_else(|| bad("truncated entry"))?;
    match e.method {
        0 => Ok(Some(data.to_vec())),
        8 => {
            let mut out = Vec::with_capacity(e.size.min(MAX_ENTRY_BYTES));
            DeflateDecoder::new(data)
                .take(MAX_ENTRY_BYTES as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|_| bad("damaged compressed data"))?;
            if out.len() > MAX_ENTRY_BYTES {
                return Err(bad("an entry is too large"));
            }
            Ok(Some(out))
        }
        _ => Err(bad("unsupported compression")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::testing::synthetic_zip as write_zip;

    #[test]
    fn reads_back_what_it_wrote_and_refuses_junk() {
        let z = write_zip(&[("a.txt", b"alpha"), ("dir/b.xml", b"<b>beta</b>")]);
        assert_eq!(
            read_entry(&z, "dir/b.xml").unwrap().unwrap(),
            b"<b>beta</b>"
        );
        assert_eq!(read_entry(&z, "a.txt").unwrap().unwrap(), b"alpha");
        assert!(read_entry(&z, "missing").unwrap().is_none());
        assert!(read_entry(b"not a zip at all, sorry", "a").is_err());
    }
}
