//! `import_text_to_pack`: pasted text or a PDF text layer becomes a user-import pack.
//!
//! Chunk by headings and stat-block patterns, let the model convert each chunk to content JSON,
//! validate every element (schema plus dangling references) with the repair loop, give every
//! element a deterministic id, and report per chunk what was converted, needs review or failed.

pub mod anything;
mod chunk;
mod ids;
pub mod layout;
pub mod office;
mod pack;
mod pdf;
mod pdf_fonts;
mod pdf_split;
pub mod report;
#[doc(hidden)]
pub mod testing;
mod zip;

use std::collections::HashSet;
use std::sync::Arc;

use borealis_ai::task::homebrew::{ConvertInput, ConvertText};
use borealis_ai::task::{run_task, AiRunner, RunOptions};
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::MonsterData;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AssistError;
use crate::homebrew::{ai_ruleset, PackContext, StoreValidator};

pub use chunk::{chunk_text, Chunk, Hint};
pub use ids::{make_id, slug};
pub use office::{office_text, OfficeFormat};
pub use pack::{assemble, ImportedPack, Provenance, CONTENT_FILE};
pub use pdf::{extract_pdf_text, read_pdf_layout, PdfLayout, PdfPage, PdfText, MIN_CHARS_PER_PAGE};
pub use pdf_split::{split_pdf, OversizedPage, PdfPart};
pub use report::{ImportReport, ReportEntry, ReportIssue, REPORT_FILE, REPORT_FORMAT};

/// A chunk shorter than this with no recognised pattern is skipped as prose.
const MIN_UNKNOWN_CHARS: usize = 80;

/// Where the text comes from.
#[derive(Debug, Clone)]
pub enum ImportSource {
    /// Pasted text.
    Text(String),
    /// The bytes of a PDF with a text layer.
    Pdf(Vec<u8>),
}

/// Request for [`import_text_to_pack`].
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// The text or PDF.
    pub source: ImportSource,
    /// User prefix for ids (`ID_<PREFIX>_<TYPE>_<NAME>`).
    pub id_prefix: String,
    /// `source` name stamped on every element.
    pub source_name: String,
    /// Pack id.
    pub pack_id: String,
    /// Pack display name.
    pub pack_name: String,
}

/// Outcome of one chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkStatus {
    /// Valid elements, no concerns.
    Converted,
    /// Valid elements, but the user should look at them (see `issues`).
    NeedsReview,
    /// No usable element after the repair loop.
    Failed,
    /// Prose with no recognisable content; no model call was made.
    Skipped,
}

/// Per-chunk report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkReport {
    /// Chunk position.
    pub index: usize,
    /// Heading, if any.
    pub heading: Option<String>,
    /// 1-based first source line.
    pub start_line: usize,
    /// 1-based last source line.
    pub end_line: usize,
    /// Outcome.
    pub status: ChunkStatus,
    /// Ids of the elements produced.
    pub element_ids: Vec<String>,
    /// Review notes or failure reasons.
    pub issues: Vec<String>,
    /// Model attempts used (0 when skipped).
    pub attempts: u32,
}

/// Extraction summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extraction {
    /// `"text"` or `"pdf"`.
    pub source_kind: String,
    /// PDF page count.
    pub pages: Option<usize>,
    /// Characters of text handed to the chunker.
    pub chars: usize,
}

/// Result of an import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportOutcome {
    /// The assembled pack; `None` when no element was converted.
    pub pack: Option<ImportedPack>,
    /// One report per chunk.
    pub chunks: Vec<ChunkReport>,
    /// Extraction summary.
    pub extraction: Extraction,
}

impl ImportOutcome {
    /// Number of chunks with `status`.
    pub fn count(&self, status: ChunkStatus) -> usize {
        self.chunks.iter().filter(|c| c.status == status).count()
    }
}

/// Convert text or a PDF into a user-import pack.
///
/// # Errors
/// [`AssistError::Extraction`] for unreadable or text-less PDFs (no OCR is attempted). Chunk
/// failures are reported in the outcome, never as an error.
pub async fn import_text_to_pack(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    req: &ImportRequest,
    opts: &RunOptions<'_>,
) -> Result<ImportOutcome, AssistError> {
    let (text, source_kind, pages) = match &req.source {
        ImportSource::Text(t) => (t.clone(), "text", None),
        ImportSource::Pdf(b) => {
            let p = extract_pdf_text(b)?;
            (p.text, "pdf", Some(p.pages))
        }
    };
    let extraction = Extraction {
        source_kind: source_kind.to_string(),
        pages,
        chars: text.chars().count(),
    };
    let mut used: HashSet<String> = ctx
        .base
        .clone()
        .build()
        .iter()
        .map(|s| s.element.id.clone())
        .collect();
    let mut files: Vec<ContentFile> = Vec::new();
    let mut reports = Vec::new();
    for chunk in chunk_text(&text) {
        let (report, file) = convert_chunk(runner, ctx, req, opts, &chunk, &files, &mut used).await;
        reports.push(report);
        files.extend(file);
    }
    let elements: Vec<_> = files.into_iter().flat_map(|f| f.elements).collect();
    let pack = (!elements.is_empty()).then(|| {
        assemble(
            &req.pack_id,
            &req.pack_name,
            ctx.ruleset,
            source_kind,
            ContentFile::new(elements),
        )
    });
    Ok(ImportOutcome {
        pack,
        chunks: reports,
        extraction,
    })
}

fn report(chunk: &Chunk, status: ChunkStatus, attempts: u32, issues: Vec<String>) -> ChunkReport {
    ChunkReport {
        index: chunk.index,
        heading: chunk.heading.clone(),
        start_line: chunk.start_line,
        end_line: chunk.end_line,
        status,
        element_ids: Vec::new(),
        issues,
        attempts,
    }
}

async fn convert_chunk(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    req: &ImportRequest,
    opts: &RunOptions<'_>,
    chunk: &Chunk,
    done: &[ContentFile],
    used: &mut HashSet<String>,
) -> (ChunkReport, Option<ContentFile>) {
    if chunk.hint == Hint::Unknown && chunk.text.chars().count() < MIN_UNKNOWN_CHARS {
        let why = vec![
            "no recognisable content (too short and no stat block, spell or item pattern)"
                .to_string(),
        ];
        return (report(chunk, ChunkStatus::Skipped, 0, why), None);
    }
    let mut base = ctx.base.clone();
    for f in done {
        base = base.add_file("import", PackOrigin::Custom, f.clone());
    }
    let validator = StoreValidator::new(PackContext {
        base,
        ruleset: ctx.ruleset,
    });
    let input = ConvertInput {
        text: chunk.text.clone(),
        kind: None,
        ruleset: ai_ruleset(ctx.ruleset),
        source_name: req.source_name.clone(),
        existing_ids: used.iter().cloned().collect(),
    };
    let task = ConvertText::new(input).with_validator(Arc::new(validator));
    let result = match run_task(runner, &task, opts).await {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("conversion failed: {e}");
            return (
                report(chunk, ChunkStatus::Failed, opts.max_repairs + 1, vec![msg]),
                None,
            );
        }
    };
    let attempts = result.attempts;
    let mut content = result.output.content;
    let mut issues = result.output.warnings;
    let mapping = ids::reassign_ids(&mut content, &req.id_prefix, used);
    stamp_source(&mut content, &req.source_name);
    let file = match ContentFile::from_json_str(&content.to_string()) {
        Ok(f) if !f.elements.is_empty() => f,
        Ok(_) => {
            let r = report(
                chunk,
                ChunkStatus::Failed,
                attempts,
                vec!["the model returned no elements".into()],
            );
            return (r, None);
        }
        Err(e) => {
            let r = report(
                chunk,
                ChunkStatus::Failed,
                attempts,
                vec![format!("result does not load: {e}")],
            );
            return (r, None);
        }
    };
    issues.extend(review_notes(chunk, &file));
    let status = if issues.is_empty() {
        ChunkStatus::Converted
    } else {
        ChunkStatus::NeedsReview
    };
    let mut r = report(chunk, status, attempts, issues);
    r.element_ids = mapping.into_iter().map(|(_, n)| n).collect();
    (r, Some(file))
}

fn stamp_source(content: &mut Value, source: &str) {
    if let Some(els) = content.get_mut("elements").and_then(Value::as_array_mut) {
        for el in els {
            el["source"] = Value::String(source.to_string());
        }
    }
}

/// Deterministic checks that put a valid chunk into the review list.
fn review_notes(chunk: &Chunk, file: &ContentFile) -> Vec<String> {
    let mut notes = Vec::new();
    let expected = chunk.hint.types();
    if !expected.is_empty()
        && !file
            .elements
            .iter()
            .any(|e| expected.contains(&e.ty.as_str()))
    {
        notes.push(format!(
            "the text looks like {:?} but no element of type {:?} was produced",
            chunk.hint, expected
        ));
    }
    for el in file.elements.iter().filter(|e| e.ty == "Monster") {
        match MonsterData::from_element(el) {
            Ok(m) => notes.extend(
                m.soft_checks()
                    .into_iter()
                    .map(|i| format!("{}: {}", el.name, i.message)),
            ),
            Err(e) => notes.push(format!("{}: stat block data does not parse: {e}", el.name)),
        }
    }
    notes
}
