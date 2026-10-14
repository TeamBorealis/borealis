//! Reading attachments into the source text, without condensing anything.
//!
//! * A PDF page with a text layer contributes that text (layout-aware: headings, columns).
//! * Pages without one (scans) and images are sent to the model a few pages at a time; a PDF of
//!   any size is split into parts under the model's limits first. Each part is transcribed
//!   verbatim and its coverage is checked: every heading the model lists for the part must appear
//!   in its text, else the part is asked once more; what is still missing is reported.
//! * Parts are joined in page order, so nothing is summarised and nothing is dropped silently.

use borealis_ai::attachment::MAX_PDF_BYTES;
use borealis_ai::task::{run_task, RunOptions};
use borealis_ai::{Attachment, AttachmentKind};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::steps::{detect_step, Attaching, DetectOut, Step, VisionRunner};
use super::types::{DetectedItem, Edition};
use crate::error::AssistError;
use crate::import::chunk::{headings, missing_headings, norm_words};
use crate::import::{read_pdf_layout, split_pdf, PdfLayout};

/// Pages (or images) the model transcribes per call: few enough to copy verbatim.
pub(crate) const VISUAL_PAGES_PER_CALL: usize = 4;
/// Largest PDF part sent (below the 20 MB attachment limit, leaving room for base64).
pub(crate) const MAX_PART_BYTES: usize = 15 * 1024 * 1024;
/// Extra transcription attempts when listed headings are missing from the text.
const COVERAGE_RETRIES: usize = 1;
/// Most source characters one detection call reads; a longer source is detected window by
/// window (cut at headings) and the findings are merged.
pub(crate) const DETECT_WINDOW_CHARS: usize = 120_000;
/// Prefix of the notes ingestion adds to the detection (they also go to the import report).
pub const READING_NOTE: &str = "Reading the source: ";

/// The source text read from attachments, with what could not be read.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Ingested {
    /// Text in reading order, headings as `#` lines.
    pub text: String,
    /// Problems (pages too large, headings missing from a transcription).
    pub notes: Vec<String>,
    /// Model calls made.
    pub calls: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TranscribeOut {
    pub text: String,
    #[serde(default)]
    pub headings: Vec<String>,
}

fn transcribe_step(label: &str, missing: &[String]) -> Step<TranscribeOut> {
    let again = if missing.is_empty() {
        String::new()
    } else {
        format!(
            "\nA previous transcription left out these headings and their text: {}. Transcribe everything again, completely.",
            missing.join("; ")
        )
    };
    let user = format!(
        "Transcribe {label} completely and verbatim, in reading order (a two-column page: the left \
column, then the right column). Do NOT summarise, shorten, merge or skip anything: every heading, \
paragraph, list item, sidebar, table row and stat block line. Mark headings with Markdown `#`, `##`, \
`###` by their printed size. Write each table row as `cell | cell | cell`. Leave out running page \
headers, footers and page numbers. In `headings` list every heading on these pages exactly as \
printed.{again}"
    );
    Step::new(
        "import_transcribe",
        "transcribe the attached pages of a D&D source verbatim",
        "Output the text exactly as printed; never paraphrase.",
        user,
        json!({"type":"object","required":["text"],"properties":{
            "text":{"type":"string"},
            "headings":{"type":"array","items":{"type":"string"}}}}),
    )
}

/// One visual unit: attachments the model reads in one call.
struct Visual {
    label: String,
    attachments: Vec<Attachment>,
}

/// One piece of the source in reading order.
enum Piece {
    Text(String),
    Visual(Visual),
}

async fn transcribe(
    runner: &dyn VisionRunner,
    v: &Visual,
    opts: &RunOptions<'_>,
    out: &mut Ingested,
) -> Result<String, AssistError> {
    let adapter = Attaching {
        inner: runner,
        attachments: &v.attachments,
    };
    let mut missing: Vec<String> = Vec::new();
    let mut best: Option<(usize, String)> = None;
    for _ in 0..=COVERAGE_RETRIES {
        out.calls += 1;
        let reply = run_task(&adapter, &transcribe_step(&v.label, &missing), opts)
            .await?
            .output;
        let gaps = missing_headings(&reply.headings, &reply.text);
        if best.as_ref().map_or(true, |(n, _)| gaps.len() < *n) {
            best = Some((gaps.len(), reply.text));
        }
        missing = gaps;
        if missing.is_empty() {
            break;
        }
    }
    if !missing.is_empty() {
        out.notes.push(format!(
            "{READING_NOTE}{}: these headings were seen but their text could not be transcribed: {}",
            v.label,
            missing.join("; ")
        ));
    }
    let text = best.map(|(_, t)| t).unwrap_or_default();
    if text.trim().is_empty() {
        out.notes
            .push(format!("{READING_NOTE}{}: no text could be read", v.label));
    }
    Ok(text)
}

fn page_label(name: &str, first: u32, last: u32) -> String {
    if first == last {
        format!("page {first} of `{name}`")
    } else {
        format!("pages {first}-{last} of `{name}`")
    }
}

/// Pieces of one PDF: text-layer runs and visual parts, in page order.
fn pdf_pieces(a: &Attachment, notes: &mut Vec<String>) -> Result<Vec<Piece>, AssistError> {
    let layout = match read_pdf_layout(&a.bytes) {
        Ok(l) => l,
        // Unparseable here but maybe readable by the model: send it whole when it fits.
        Err(e) if a.bytes.len() <= MAX_PDF_BYTES => {
            notes.push(format!(
                "{READING_NOTE}`{}` has no readable structure ({e}); the model reads it as a whole",
                a.name
            ));
            return Ok(vec![Piece::Visual(Visual {
                label: format!("`{}`", a.name),
                attachments: vec![a.clone()],
            })]);
        }
        Err(e) => return Err(e),
    };
    let scale = layout.scale();
    let mut pieces = Vec::new();
    let mut i = 0;
    while i < layout.pages.len() {
        let text = layout.pages[i].has_text();
        let mut j = i;
        while j < layout.pages.len() && layout.pages[j].has_text() == text {
            j += 1;
        }
        let run: Vec<usize> = (i..j).collect();
        if text {
            pieces.push(Piece::Text(layout.text_of(&run, &scale)));
        } else {
            pieces.extend(visual_parts(a, &layout, &run, notes)?);
        }
        i = j;
    }
    Ok(pieces)
}

fn visual_parts(
    a: &Attachment,
    layout: &PdfLayout,
    run: &[usize],
    notes: &mut Vec<String>,
) -> Result<Vec<Piece>, AssistError> {
    let numbers: Vec<u32> = run.iter().map(|&k| layout.pages[k].number).collect();
    let (parts, oversized) = split_pdf(&a.bytes, &numbers, VISUAL_PAGES_PER_CALL, MAX_PART_BYTES)?;
    for o in oversized {
        notes.push(format!(
            "{READING_NOTE}page {} of `{}` is {} MB on its own, over the {} MB a model call takes; it was not read",
            o.page,
            a.name,
            o.bytes / (1024 * 1024),
            MAX_PART_BYTES / (1024 * 1024)
        ));
    }
    Ok(parts
        .into_iter()
        .map(|p| {
            let label = page_label(&a.name, p.first_page, p.last_page);
            Piece::Visual(Visual {
                attachments: vec![Attachment {
                    kind: AttachmentKind::Pdf,
                    bytes: p.bytes,
                    name: format!("{} ({label})", a.name),
                }],
                label,
            })
        })
        .collect())
}

/// Read `attachments` into one source text.
///
/// # Errors
/// [`AssistError::Ai`] when a transcription call fails, [`AssistError::Extraction`] for a PDF
/// that can neither be parsed nor sent whole, or when nothing could be read.
pub(crate) async fn read_attachments(
    runner: &dyn VisionRunner,
    attachments: &[Attachment],
    opts: &RunOptions<'_>,
) -> Result<Ingested, AssistError> {
    let mut out = Ingested::default();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut images: Vec<Attachment> = Vec::new();
    let flush = |images: &mut Vec<Attachment>, pieces: &mut Vec<Piece>| {
        for group in images.chunks(VISUAL_PAGES_PER_CALL) {
            let names: Vec<&str> = group.iter().map(|a| a.name.as_str()).collect();
            pieces.push(Piece::Visual(Visual {
                label: format!("the attached image(s) {}", names.join(", ")),
                attachments: group.to_vec(),
            }));
        }
        images.clear();
    };
    for a in attachments {
        match a.kind {
            AttachmentKind::Image { .. } => images.push(a.clone()),
            AttachmentKind::Pdf => {
                flush(&mut images, &mut pieces);
                pieces.extend(pdf_pieces(a, &mut out.notes)?);
            }
        }
    }
    flush(&mut images, &mut pieces);
    let mut texts = Vec::new();
    for p in &pieces {
        let t = match p {
            Piece::Text(t) => t.clone(),
            Piece::Visual(v) => transcribe(runner, v, opts, &mut out).await?,
        };
        if !t.trim().is_empty() {
            texts.push(t.trim().to_string());
        }
    }
    out.text = texts.join("\n\n");
    if out.text.trim().is_empty() {
        return Err(AssistError::Extraction(
            "the model could not read any text in the attachments".into(),
        ));
    }
    Ok(out)
}

/// `text` cut into windows of at most `max` characters, at headings where possible (a window
/// with no heading to cut at is cut at a line).
pub(crate) fn windows(text: &str, max: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let heads: std::collections::HashSet<usize> =
        headings(text).into_iter().map(|h| h.line).collect();
    let mut out = Vec::new();
    let (mut start, mut size, mut last_head) = (0usize, 0usize, None::<usize>);
    for (i, l) in lines.iter().enumerate() {
        if heads.contains(&i) && i > start {
            last_head = Some(i);
        }
        size += l.chars().count() + 1;
        if size > max && i > start {
            let cut = last_head.filter(|h| *h > start).unwrap_or(i);
            out.push(lines[start..cut].join("\n"));
            start = cut;
            size = lines[start..=i].iter().map(|l| l.chars().count() + 1).sum();
            last_head = None;
        }
    }
    if start < lines.len() {
        out.push(lines[start..].join("\n"));
    }
    out.retain(|w| !w.trim().is_empty());
    out
}

fn merge_detections(outs: Vec<DetectOut>) -> Option<DetectOut> {
    let mut iter = outs.into_iter();
    let mut merged = iter.next()?;
    let mut votes: Vec<Edition> = vec![merged.edition];
    for o in iter {
        votes.push(o.edition);
        merged.confidence = merged.confidence.min(o.confidence);
        for item in o.items {
            add_item(&mut merged.items, item);
        }
        for other in o.other {
            if !merged
                .other
                .iter()
                .any(|x| norm_words(&x.name) == norm_words(&other.name))
            {
                merged.other.push(other);
            }
        }
        merged.notes.extend(o.notes);
    }
    let later = votes.iter().filter(|e| **e != votes[0]).count();
    if later * 2 > votes.len() {
        merged.edition = votes
            .iter()
            .copied()
            .find(|e| *e != votes[0])
            .unwrap_or(merged.edition);
    }
    Some(merged)
}

fn add_item(items: &mut Vec<DetectedItem>, item: DetectedItem) {
    let key = norm_words(&item.name);
    match items
        .iter_mut()
        .find(|i| i.kind == item.kind && norm_words(&i.name) == key)
    {
        Some(found) => {
            found.parent_hint = found.parent_hint.take().or(item.parent_hint);
            found.item_type = found.item_type.or(item.item_type);
        }
        None => items.push(item),
    }
}

/// Detect over `text`, window by window when it is long.
///
/// # Errors
/// [`AssistError::Ai`] when a detection call fails.
pub(crate) async fn detect_text(
    runner: &dyn VisionRunner,
    text: &str,
    edition_hint: Edition,
    opts: &RunOptions<'_>,
) -> Result<DetectOut, AssistError> {
    let adapter = Attaching {
        inner: runner,
        attachments: &[],
    };
    let mut outs = Vec::new();
    for w in windows(text, DETECT_WINDOW_CHARS) {
        let step: Step<DetectOut> = detect_step(&w, false, edition_hint);
        outs.push(run_task(&adapter, &step, opts).await?.output);
    }
    merge_detections(outs).ok_or_else(|| AssistError::Extraction("the source is empty".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_into_windows_at_headings_without_losing_lines() {
        let mut text = String::new();
        for i in 0..40 {
            text.push_str(&format!("# Part {i}\n\n{}\n\n", "word ".repeat(60)));
        }
        let w = windows(&text, 2000);
        assert!(w.len() > 5);
        assert!(w.iter().all(|x| x.chars().count() <= 2000));
        assert!(
            w.iter().skip(1).all(|x| x.starts_with("# Part")),
            "{:?}",
            &w[1][..20]
        );
        assert_eq!(
            w.join("\n").lines().count(),
            text.trim_end().lines().count()
        );
    }
}
