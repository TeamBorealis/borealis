//! Pure-Rust PDF text extraction (no OCR).

use lopdf::Document;

use crate::error::AssistError;

/// A PDF with fewer non-whitespace characters per page than this is treated as scanned images.
pub const MIN_CHARS_PER_PAGE: usize = 20;

/// Stack for the PDF parsing thread.
const PDF_THREAD_STACK: usize = 64 * 1024 * 1024;
/// Stop collecting text once this many bytes are extracted (flate-bomb guard).
pub const MAX_EXTRACT_BYTES: usize = 8 * 1024 * 1024;

/// Text pulled out of a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfText {
    /// Extracted text, pages separated by a blank line.
    pub text: String,
    /// Page count.
    pub pages: usize,
}

/// Extract the text layer of a PDF.
///
/// # Errors
/// [`AssistError::Extraction`] when the file cannot be parsed, or yields too little text (a scanned
/// PDF); Borealis does not run OCR, so the message tells the user to paste the text instead.
pub fn extract_pdf_text(bytes: &[u8]) -> Result<PdfText, AssistError> {
    // lopdf recurses on nested objects; run on a big-stack thread so a hostile file yields an
    // error instead of aborting the process.
    let owned = bytes.to_vec();
    std::thread::Builder::new()
        .name("pdf-extract".into())
        .stack_size(PDF_THREAD_STACK)
        .spawn(move || extract_inner(&owned))
        .map_err(|e| AssistError::Extraction(format!("could not start PDF reader: {e}")))?
        .join()
        .map_err(|_| {
            AssistError::Extraction("the PDF could not be processed (parser panicked)".into())
        })?
}

fn extract_inner(bytes: &[u8]) -> Result<PdfText, AssistError> {
    let doc = Document::load_mem(bytes)
        .map_err(|e| AssistError::Extraction(format!("the PDF could not be read: {e}")))?;
    let numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
    let pages = numbers.len();
    let mut parts = Vec::new();
    let mut total = 0usize;
    for n in &numbers {
        if total >= MAX_EXTRACT_BYTES {
            break;
        }
        if let Ok(t) = doc.extract_text(&[*n]) {
            total += t.len();
            parts.push(t);
        }
    }
    let text = parts.join("\n\n");
    let chars = text.chars().filter(|c| !c.is_whitespace()).count();
    if pages == 0 || chars < MIN_CHARS_PER_PAGE * pages {
        return Err(AssistError::Extraction(format!(
            "the PDF has almost no text layer ({chars} characters over {pages} page(s)); it is probably scanned images. \
Borealis does not run OCR: paste the text instead, or run OCR in another tool first."
        )));
    }
    Ok(PdfText { text, pages })
}
