//! Split a PDF into page ranges small enough for the model (pages and bytes), so a book of any
//! size can be read part by part.

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::pdf::load_pdf;
use crate::error::AssistError;

/// Page-tree attributes a page may inherit from its parents.
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
/// Catalog entries that reference pages outside a part (they would keep them alive).
const DROPPED: [&[u8]; 6] = [
    b"Outlines",
    b"StructTreeRoot",
    b"Names",
    b"Dests",
    b"OpenAction",
    b"AcroForm",
];

/// One part of a split PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPart {
    /// 1-based first page.
    pub first_page: u32,
    /// 1-based last page.
    pub last_page: u32,
    /// The part as a PDF.
    pub bytes: Vec<u8>,
}

/// A page range that could not be made small enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OversizedPage {
    /// 1-based page number.
    pub page: u32,
    /// Its size alone.
    pub bytes: usize,
}

fn bad(e: impl std::fmt::Display) -> AssistError {
    AssistError::Extraction(format!("the PDF could not be split: {e}"))
}

/// Copy the attributes a page inherits from its page-tree parents onto the page itself.
fn flatten_inherited(doc: &mut Document, page: ObjectId) {
    let mut found: Vec<(Vec<u8>, Object)> = Vec::new();
    let mut parent = doc
        .get_dictionary(page)
        .ok()
        .and_then(|d| d.get(b"Parent").and_then(Object::as_reference).ok());
    for _ in 0..32 {
        let Some(dict) = parent.and_then(|p| doc.get_dictionary(p).ok()) else {
            break;
        };
        for key in INHERITED {
            if let Ok(v) = dict.get(key) {
                if !found.iter().any(|(k, _)| k == key) {
                    found.push((key.to_vec(), v.clone()));
                }
            }
        }
        parent = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page) {
        for (k, v) in found {
            if !d.has(&k) {
                d.set(k, v);
            }
        }
    }
}

fn root_pages(doc: &Document) -> Result<ObjectId, AssistError> {
    doc.catalog()
        .and_then(|c| c.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(bad)
}

/// A new document with only `pages` (1-based, in order) of `source`.
fn extract(source: &Document, pages: &[u32]) -> Result<Vec<u8>, AssistError> {
    let mut doc = source.clone();
    let all = doc.get_pages();
    let ids: Vec<ObjectId> = pages.iter().filter_map(|n| all.get(n).copied()).collect();
    if ids.is_empty() {
        return Err(bad("no such pages"));
    }
    let root = root_pages(&doc)?;
    for id in &ids {
        flatten_inherited(&mut doc, *id);
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(*id) {
            d.set("Parent", root);
            // Link annotations point at pages outside the part and would keep them alive.
            d.remove(b"Annots");
        }
    }
    let kids: Vec<Object> = ids.iter().map(|id| Object::Reference(*id)).collect();
    let mut tree = Dictionary::new();
    tree.set("Type", "Pages");
    tree.set("Count", i64::try_from(ids.len()).unwrap_or(i64::MAX));
    tree.set("Kids", kids);
    doc.objects.insert(root, Object::Dictionary(tree));
    if let Ok(cat) = doc.catalog_mut() {
        for key in DROPPED {
            cat.remove(key);
        }
    }
    doc.prune_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(bad)?;
    Ok(out)
}

/// Split `bytes` into parts of at most `max_pages` pages (of `pages`, 1-based, ascending) and
/// at most `max_bytes` each; a part over the byte limit is halved until it fits. Pages that
/// exceed the limit on their own are returned separately.
///
/// # Errors
/// [`AssistError::Extraction`] when the PDF cannot be read or written.
pub fn split_pdf(
    bytes: &[u8],
    pages: &[u32],
    max_pages: usize,
    max_bytes: usize,
) -> Result<(Vec<PdfPart>, Vec<OversizedPage>), AssistError> {
    let doc = load_pdf(bytes)?;
    let mut parts = Vec::new();
    let mut oversized = Vec::new();
    let mut queue: Vec<Vec<u32>> = pages
        .chunks(max_pages.max(1))
        .rev()
        .map(<[u32]>::to_vec)
        .collect();
    while let Some(range) = queue.pop() {
        let pdf = extract(&doc, &range)?;
        if pdf.len() <= max_bytes {
            parts.push(PdfPart {
                first_page: range[0],
                last_page: range[range.len() - 1],
                bytes: pdf,
            });
        } else if range.len() == 1 {
            oversized.push(OversizedPage {
                page: range[0],
                bytes: pdf.len(),
            });
        } else {
            let (a, b) = range.split_at(range.len() / 2);
            queue.push(b.to_vec());
            queue.push(a.to_vec());
        }
    }
    Ok((parts, oversized))
}

#[cfg(test)]
mod tests {
    use super::super::pdf::read_pdf_layout;
    use super::*;
    use crate::import::testing::synthetic_pdf as build;

    #[test]
    fn parts_keep_their_pages_and_respect_both_limits() {
        let pages: Vec<Vec<_>> = (0..7)
            .map(|_| vec![(72.0, 700.0, 10.0, false, "A page of invented text.")])
            .collect();
        let pdf = build(&pages, 60_000);
        let (parts, big) = split_pdf(&pdf, &[1, 2, 3, 4, 5, 6, 7], 3, 150_000).unwrap();
        assert!(big.is_empty());
        let ranges: Vec<(u32, u32)> = parts.iter().map(|p| (p.first_page, p.last_page)).collect();
        assert_eq!(ranges.first().map(|r| r.0), Some(1));
        assert_eq!(ranges.last().map(|r| r.1), Some(7));
        for w in ranges.windows(2) {
            assert_eq!(w[0].1 + 1, w[1].0, "{ranges:?}");
        }
        for p in &parts {
            assert!(p.bytes.len() <= 150_000);
            assert!(p.last_page - p.first_page < 3);
            let l = read_pdf_layout(&p.bytes).unwrap();
            assert_eq!(l.pages.len() as u32, p.last_page - p.first_page + 1);
            assert!(l.pages.iter().all(|pg| pg.has_text()));
        }
        let (_, big) = split_pdf(&pdf, &[2], 3, 1000).unwrap();
        assert_eq!(big.len(), 1);
    }
}
