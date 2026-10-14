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
/// Extra transcription attempts when listed headings are missing from the text, or the text is
/// shorter than the pages hold, or leaves out words their text layer has.
const COVERAGE_RETRIES: usize = 1;
/// A transcription shorter than this share of the characters the pages' (garbled) text layer
/// holds was condensed, not copied.
const MIN_LENGTH_SHARE: f32 = 0.75;
/// Share of the words of a page's incomplete text layer a verbatim transcription contains.
const MIN_WORD_SHARE: f32 = 0.85;
/// Words of the text layer checked per part (longest first).
const MAX_CHECK_WORDS: usize = 400;
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

fn transcribe_step(label: &str, problems: &[String]) -> Step<TranscribeOut> {
    let again = if problems.is_empty() {
        String::new()
    } else {
        format!(
            "\nA previous transcription of these pages was rejected: {}. Copy every word again, exactly as printed.",
            problems.join("; ")
        )
    };
    let user = format!(
        "Transcribe {label} completely and VERBATIM: copy the printed words one by one, in reading \
order (a two-column page: the left column, then the right column). This is copying, not writing: \
do NOT summarise, shorten, paraphrase, reword, correct, reorder, merge or skip anything; keep every \
number, die expression, name and sentence exactly as printed. Include every heading, paragraph, \
list item, sidebar, table row and stat block line. Mark headings with Markdown `#`, `##`, `###` by \
their printed size. Write each table row on one line as `cell | cell | cell`. Leave out running \
page headers, footers and page numbers. In `headings` list every heading on these pages exactly as \
printed.{again}"
    );
    Step::new(
        "import_transcribe",
        "copy the attached pages of a D&D source word for word",
        "Output the printed text exactly, word for word; never paraphrase or condense.",
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
    /// What a verbatim transcription of it must contain, from the pages' text layer.
    expect: Expect,
}

/// What a verbatim transcription is checked against: the glyph count of a garbled text layer
/// (it shows as many characters as the page prints) and the words of an incomplete one.
#[derive(Debug, Clone, Default)]
struct Expect {
    min_chars: usize,
    words: Vec<String>,
}

impl Expect {
    /// Why `text` is not a verbatim transcription of the pages; empty when it is.
    fn problems(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let chars = text.chars().filter(|c| !c.is_whitespace()).count();
        if self.min_chars > 0 && (chars as f32) < MIN_LENGTH_SHARE * self.min_chars as f32 {
            out.push(format!(
                "it has {chars} characters but the pages print about {}",
                self.min_chars
            ));
        }
        if !self.words.is_empty() {
            let hay = format!(" {} ", norm_words(text));
            let missing: Vec<&String> = self
                .words
                .iter()
                .filter(|w| !hay.contains(&format!(" {w} ")))
                .collect();
            let kept = (self.words.len() - missing.len()) as f32;
            if kept < MIN_WORD_SHARE * self.words.len() as f32 {
                let sample: Vec<&str> = missing.iter().take(12).map(|w| w.as_str()).collect();
                out.push(format!(
                    "it leaves out {} of {} printed words (such as: {})",
                    missing.len(),
                    self.words.len(),
                    sample.join(", ")
                ));
            }
        }
        out
    }

    /// The expectation for the pages `numbers` of `layout`.
    fn of(layout: &PdfLayout, numbers: std::ops::RangeInclusive<u32>) -> Self {
        let mut e = Self::default();
        let mut words: Vec<String> = Vec::new();
        for p in layout.pages.iter().filter(|p| numbers.contains(&p.number)) {
            if p.garbled() {
                e.min_chars += p.chars();
            } else {
                for l in &p.lines {
                    words.extend(
                        norm_words(&l.text)
                            .split(' ')
                            .filter(|w| {
                                w.chars().count() >= 4 && w.chars().all(char::is_alphabetic)
                            })
                            .map(str::to_string),
                    );
                }
            }
        }
        words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
        words.dedup();
        words.truncate(MAX_CHECK_WORDS);
        e.words = words;
        e
    }
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
    let mut problems: Vec<String> = Vec::new();
    let mut best: Option<(usize, Vec<String>, String)> = None;
    for _ in 0..=COVERAGE_RETRIES {
        out.calls += 1;
        let reply = run_task(&adapter, &transcribe_step(&v.label, &problems), opts)
            .await?
            .output;
        let gaps = missing_headings(&reply.headings, &reply.text);
        let mut found: Vec<String> = Vec::new();
        if !gaps.is_empty() {
            found.push(format!(
                "these headings were seen but their text is missing: {}",
                gaps.join("; ")
            ));
        }
        found.extend(v.expect.problems(&reply.text));
        let score = gaps.len() + found.len();
        if best.as_ref().map_or(true, |(n, _, _)| score < *n) {
            best = Some((score, found.clone(), reply.text));
        }
        problems = found;
        if problems.is_empty() {
            break;
        }
    }
    let kept = best.as_ref().map(|(_, p, _)| p.clone()).unwrap_or_default();
    if !kept.is_empty() {
        out.notes.push(format!(
            "{READING_NOTE}{} may not be verbatim after {} attempts: {}",
            v.label,
            COVERAGE_RETRIES + 1,
            kept.join("; ")
        ));
    }
    let (text, paraphrased) = strip_paraphrase(&best.map(|(_, _, t)| t).unwrap_or_default());
    if paraphrased {
        out.notes.push(format!(
            "{READING_NOTE}{}: the visual transcription says it is not a word-for-word copy (paraphrased passages), so descriptions from these pages are not the printed text; import a text export of the source (ODT or DOCX) to keep the printed wording",
            v.label
        ));
    }
    if text.trim().is_empty() {
        out.notes
            .push(format!("{READING_NOTE}{}: no text could be read", v.label));
    }
    Ok(text)
}

/// Phrases of a transcription's own disclaimer that it did not copy the page word for word.
const DISCLAIMER_MARKS: [&str; 6] = [
    "word-for-word",
    "word for word",
    "not verbatim",
    "can't reproduce",
    "cannot reproduce",
    "paraphrased",
];
/// Paragraphs at the start of a transcription searched for its disclaimer.
const DISCLAIMER_PARAGRAPHS: usize = 2;
/// Inline markers a transcription puts on passages it reworded.
const PARAPHRASE_MARKS: [&str; 4] = [
    "[paraphrased]",
    "[paraphrased.]",
    "(paraphrased)",
    "(paraphrased.)",
];

/// A transcription without its "not a word-for-word copy" disclaimer paragraph and its
/// "(Paraphrased.)" passage markers, and whether it had either (its text is then not the
/// printed text). Headings are never a disclaimer.
fn strip_paraphrase(text: &str) -> (String, bool) {
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let disclaimer = paragraphs.iter().take(DISCLAIMER_PARAGRAPHS).position(|p| {
        let lower = p.to_lowercase();
        !p.trim_start().starts_with('#') && DISCLAIMER_MARKS.iter().any(|m| lower.contains(m))
    });
    let kept: Vec<&str> = paragraphs
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != disclaimer)
        .map(|(_, p)| *p)
        .collect();
    let mut out = kept.join("\n\n");
    let mut marked = false;
    for mark in PARAPHRASE_MARKS {
        while let Some(at) = out.to_lowercase().find(mark) {
            let end = at + mark.len();
            let skip = out[end..].len() - out[end..].trim_start_matches([' ', '\t']).len();
            out.replace_range(at..end + skip, "");
            marked = true;
        }
    }
    (out, marked || disclaimer.is_some())
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
                expect: Expect::default(),
            })]);
        }
        Err(e) => return Err(e),
    };
    let scale = layout.scale();
    let reasons: Vec<String> = layout
        .pages
        .iter()
        .filter_map(|p| {
            p.distrust
                .as_ref()
                .map(|r| format!("page {}: {r}", p.number))
        })
        .collect();
    if !reasons.is_empty() {
        notes.push(format!(
            "{READING_NOTE}`{}`: {} page(s) were read visually because their text layer cannot be used ({})",
            a.name,
            reasons.len(),
            reasons.join("; ")
        ));
    }
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
                expect: Expect::of(layout, p.first_page..=p.last_page),
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
                expect: Expect::default(),
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

    #[test]
    fn a_paraphrase_disclaimer_and_markers_are_stripped_and_flagged() {
        let t = "> **Note:** This is not a word-for-word copy of the pages.\n\n# Mossfolk\n[Paraphrased] Mossfolk live in bogs.\n\n**Age.** Adult at 20. (Paraphrased.)";
        let (text, flagged) = strip_paraphrase(t);
        assert!(flagged);
        assert_eq!(
            text,
            "# Mossfolk\nMossfolk live in bogs.\n\n**Age.** Adult at 20. "
        );
        // A heading's paragraph is never the disclaimer.
        let (_, flagged) = strip_paraphrase("# Mossfolk\nThey repeat oaths word for word.");
        assert!(!flagged);
        let plain = "# Mossfolk\n\nMossfolk live in bogs.";
        assert_eq!(strip_paraphrase(plain), (plain.to_string(), false));
    }

    #[test]
    fn condensed_or_reworded_transcriptions_are_rejected() {
        let garbled = Expect {
            min_chars: 400,
            words: Vec::new(),
        };
        assert!(!garbled.problems("A short summary of the page.").is_empty());
        assert!(garbled.problems(&"word ".repeat(120)).is_empty());
        let partial = Expect {
            min_chars: 0,
            words: ["lantern", "hollow", "glimmer", "spire"]
                .map(String::from)
                .to_vec(),
        };
        assert!(partial
            .problems("The glimmer of a lantern in the hollow by the spire.")
            .is_empty());
        let p = partial.problems("A light in a cave near a tower.");
        assert!(p[0].contains("leaves out 4 of 4"), "{p:?}");
    }

    /// Offline measurement on a real import (no model calls): `BOREALIS_PROBE=<draft.json>`,
    /// optional `BOREALIS_PROBE_PDF=<a.pdf>[,<b.pdf>]` to rebuild the source from the PDF text
    /// layer (pages read visually are left out). Prints counts only.
    #[test]
    #[ignore = "needs a local draft.json"]
    fn probe_locating_and_planned_calls() {
        use super::super::types::{ElementStatus, ImportDraft, Role};
        let Ok(path) = std::env::var("BOREALIS_PROBE") else {
            return;
        };
        let mut draft: ImportDraft =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        if let Ok(pdfs) = std::env::var("BOREALIS_PROBE_PDF") {
            let mut texts = Vec::new();
            for f in pdfs.split(',') {
                let a = Attachment {
                    kind: AttachmentKind::Pdf,
                    bytes: std::fs::read(f).unwrap(),
                    name: "probe.pdf".into(),
                };
                for p in pdf_pieces(&a, &mut Vec::new()).unwrap() {
                    if let Piece::Text(t) = p {
                        texts.push(t.trim().to_string());
                    }
                }
            }
            draft.source_text = texts.join("\n\n");
            if let Ok(dump) = std::env::var("BOREALIS_PROBE_DUMP") {
                std::fs::write(dump, &draft.source_text).unwrap();
            }
        }
        let src = draft.source_text.clone();
        let (mut located, mut one_line) = (0, 0);
        let mut listing = String::new();
        // `BOREALIS_PROBE_TYPED=1`: locate with each element's type (equipment prefers its row).
        let typed = std::env::var("BOREALIS_PROBE_TYPED").is_ok();
        for p in &mut draft.plan.elements {
            let ty = if typed { p.ty.clone() } else { String::new() };
            let at = |name: &str| super::super::locate::locate_section_as(&src, "", name, &ty);
            p.excerpt =
                at(&p.name).or_else(|| super::super::variants::base_name(&p.name).and_then(at));
            let verbose = std::env::var("BOREALIS_PROBE_VERBOSE").is_ok();
            let watch = std::env::var("BOREALIS_PROBE_NAMES").unwrap_or_default();
            if watch.split(',').any(|w| w == p.name) {
                println!("WATCH {:?}: {:?}", p.name, p.excerpt);
            }
            let span = p.excerpt.as_ref().map(|e| (e.start_line, e.end_line));
            listing.push_str(&format!("{}\t{:?}\t{span:?}\n", p.name, p.role));
            match &p.excerpt {
                Some(ex) => {
                    located += 1;
                    one_line += usize::from(ex.end_line <= ex.start_line);
                    if verbose && ex.end_line <= ex.start_line {
                        println!("ONE {:?}: {}", p.name, ex.snippet);
                    }
                }
                None if verbose => println!("MISS {:?} ({:?})", p.name, p.role),
                None => {}
            }
        }
        if let Ok(out) = std::env::var("BOREALIS_PROBE_LIST") {
            std::fs::write(out, listing).unwrap();
        }
        for e in &mut draft.elements {
            e.status = ElementStatus::Pending;
        }
        let batches = super::super::batch::plan_batches(&draft);
        let batched: Vec<&str> = batches.iter().flatten().map(|p| p.id.as_str()).collect();
        let singles = draft
            .plan
            .elements
            .iter()
            .filter(|p| !batched.contains(&p.id.as_str()))
            .filter(|p| {
                p.role != Role::SubclassGate && !(p.role == Role::Text && p.excerpt.is_some())
            })
            .count();
        // Every element's window (single generation) or section (a batch's per-element source).
        for (var, whole) in [
            ("BOREALIS_PROBE_WINDOWS", true),
            ("BOREALIS_PROBE_SECTIONS", false),
        ] {
            let Ok(out) = std::env::var(var) else {
                continue;
            };
            let all: Vec<String> = draft
                .plan
                .elements
                .iter()
                .map(|p| {
                    let text = if whole {
                        super::super::source::element_window(&src, p)
                    } else {
                        super::super::source::section_text(&src, p).unwrap_or_default()
                    };
                    format!("===== {} ({:?})\n{text}", p.name, p.role)
                })
                .collect();
            std::fs::write(out, all.join("\n")).unwrap();
        }
        if let Ok(out) = std::env::var("BOREALIS_PROBE_ROOT") {
            let class = draft.plan.elements.iter().find(|p| p.role == Role::Class);
            if let Some(p) = class {
                std::fs::write(out, super::super::generate::source_window(&src, p)).unwrap();
            }
        }
        let roots: Vec<usize> = draft
            .plan
            .elements
            .iter()
            .filter(|p| matches!(p.role, Role::Class | Role::Subclass))
            .map(|p| {
                super::super::generate::source_window(&src, p)
                    .chars()
                    .count()
            })
            .collect();
        println!(
            "chars {} | located {located}/{} (one-line {one_line}) | batches {} + singles {singles} | root windows {roots:?}",
            src.chars().count(),
            draft.plan.elements.len(),
            batches.len(),
        );
    }
}
