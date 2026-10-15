//! Pictures on pages that do have a text layer. A stat block, table or sidebar pasted into a
//! page as an image prints text the layer does not hold: left alone, the creature's numbers are
//! nowhere in the source and later stages would have to guess them. A large image with no
//! text-layer text over it (a background or a text box drawn behind text has text over it) is
//! such a picture; its pages go to the model a few at a time, which copies only the pictures'
//! text, and [`splice`] puts each block into the page's text after the line the picture follows.

use std::collections::BTreeSet;

use borealis_ai::task::prompt::fence;
use borealis_ai::task::{run_task, RunOptions};
use borealis_ai::{Attachment, AttachmentKind};
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::super::steps::{Attaching, Step, VisionRunner};
use super::{strip_paraphrase, Ingested, MAX_PART_BYTES, READING_NOTE, VISUAL_PAGES_PER_CALL};
use crate::error::AssistError;
use crate::import::chunk::norm_words;
use crate::import::pdf_fonts::FontCache;
use crate::import::pdf_walk::{paint_page, ImageBox, Run};
use crate::import::{split_pdf, PdfLayout};

/// Smallest share of the page a picture covers (a small stat block in one column is ~8%).
const MIN_PICTURE_SHARE: f32 = 0.04;
/// Shortest side of a picture, in points (an inch): icons and text strips are smaller.
const MIN_PICTURE_SIDE: f32 = 72.0;
/// An image covering this share of the page is its background.
const BACKGROUND_SHARE: f32 = 0.85;
/// A text run this far inside a picture's edge is printed over it.
const OVERLAP_MARGIN: f32 = 4.0;
/// Page size when the page declares none (US letter).
const DEFAULT_SIZE: (f32, f32) = (612.0, 792.0);
/// Stack of the PDF walking thread (lopdf recurses on nested objects).
const WALK_STACK: usize = 64 * 1024 * 1024;
/// Most bytes of text decoded while looking for pictures (flate-bomb guard).
const WALK_BUDGET: usize = 8 * 1024 * 1024;
/// A block whose words are at least this share in the page's text layer repeats that layer.
const ECHO_SHARE: f32 = 0.9;
/// Fewest checked words before a block is judged an echo.
const ECHO_MIN_WORDS: usize = 3;
/// Longest anchor prefix tried when the whole anchor line is not found.
const ANCHOR_PREFIX_CHARS: usize = 40;

/// Pictures copied for one call: the pages, their text layers and the attachment.
pub(super) struct PicturePart {
    label: String,
    pages: Vec<(u32, String)>,
    attachment: Attachment,
}

/// The text of one picture and where it goes.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(super) struct Inset {
    pub page: u32,
    /// The text-layer line the picture follows (empty: the top of the page).
    #[serde(default)]
    pub after: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PicturesOut {
    #[serde(default)]
    blocks: Vec<Inset>,
}

/// A picture: large, not the page's background, and no text-layer text over it.
fn is_picture(b: &ImageBox, runs: &[Run], size: (f32, f32)) -> bool {
    let area = b.width() * b.height();
    let page = (size.0 * size.1).max(1.0);
    if b.width() < MIN_PICTURE_SIDE
        || b.height() < MIN_PICTURE_SIDE
        || area < MIN_PICTURE_SHARE * page
        || area >= BACKGROUND_SHARE * page
    {
        return false;
    }
    !runs.iter().any(|r| {
        let x = (r.x + r.end) / 2.0;
        !r.text.trim().is_empty()
            && x > b.x0 + OVERLAP_MARGIN
            && x < b.x1 - OVERLAP_MARGIN
            && r.y > b.y0 + OVERLAP_MARGIN
            && r.y < b.y1 - OVERLAP_MARGIN
    })
}

fn num(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn page_size(doc: &Document, page: ObjectId) -> (f32, f32) {
    let mut id = Some(page);
    for _ in 0..32 {
        let Some(dict) = id.and_then(|i| doc.get_dictionary(i).ok()) else {
            break;
        };
        if let Ok(Object::Array(b)) = dict
            .get(b"MediaBox")
            .and_then(|o| doc.dereference(o).map(|x| x.1))
        {
            let (w, h) = match b.as_slice() {
                [x0, y0, x1, y1] => ((num(x1) - num(x0)).abs(), (num(y1) - num(y0)).abs()),
                _ => (0.0, 0.0),
            };
            if w > 0.0 && h > 0.0 {
                return (w, h);
            }
        }
        id = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    DEFAULT_SIZE
}

/// The pages of `wanted` (1-based) that show at least one picture; none when the PDF cannot be
/// walked (its text layer was read, so the import goes on without the pictures).
pub(super) fn pictured_pages(bytes: &[u8], wanted: &BTreeSet<u32>) -> BTreeSet<u32> {
    if wanted.is_empty() {
        return BTreeSet::new();
    }
    let owned = bytes.to_vec();
    let wanted = wanted.clone();
    let walk = move || -> BTreeSet<u32> {
        let Ok(doc) = Document::load_mem(&owned) else {
            return BTreeSet::new();
        };
        let mut cache = FontCache::default();
        let mut budget = WALK_BUDGET;
        doc.get_pages()
            .into_iter()
            .filter(|(n, _)| wanted.contains(n))
            .filter(|(_, id)| {
                let painted = paint_page(&doc, *id, &mut cache, &mut budget);
                let size = page_size(&doc, *id);
                painted
                    .images
                    .iter()
                    .any(|b| is_picture(b, &painted.runs, size))
            })
            .map(|(n, _)| n)
            .collect()
    };
    std::thread::Builder::new()
        .name("pdf-pictures".into())
        .stack_size(WALK_STACK)
        .spawn(walk)
        .ok()
        .and_then(|h| h.join().ok())
        .unwrap_or_default()
}

fn pages_label(name: &str, pages: &[u32]) -> String {
    let list: Vec<String> = pages.iter().map(u32::to_string).collect();
    let word = if pages.len() == 1 { "page" } else { "pages" };
    format!("the pictures on {word} {} of `{name}`", list.join(", "))
}

/// The calls that copy the pictures of `pages` (layout indices, all with a text layer).
pub(super) fn picture_parts(
    a: &Attachment,
    layout: &PdfLayout,
    pages: &[usize],
    notes: &mut Vec<String>,
) -> Result<Vec<PicturePart>, AssistError> {
    let scale = layout.scale();
    let numbers: Vec<u32> = pages.iter().map(|&k| layout.pages[k].number).collect();
    let (parts, oversized) = split_pdf(&a.bytes, &numbers, VISUAL_PAGES_PER_CALL, MAX_PART_BYTES)?;
    for o in oversized {
        notes.push(format!(
            "{READING_NOTE}the pictures on page {} of `{}` were not read: the page is over the {} MB a model call takes; text printed in them (such as a stat block) is not in the source",
            o.page,
            a.name,
            MAX_PART_BYTES / (1024 * 1024)
        ));
    }
    Ok(parts
        .into_iter()
        .map(|p| {
            // A part is a run of `pages` in order (split_pdf halves a list, never reorders it).
            let own: Vec<(u32, String)> = pages
                .iter()
                .filter(|&&k| (p.first_page..=p.last_page).contains(&layout.pages[k].number))
                .map(|&k| (layout.pages[k].number, layout.text_of(&[k], &scale)))
                .collect();
            let nums: Vec<u32> = own.iter().map(|(n, _)| *n).collect();
            let label = pages_label(&a.name, &nums);
            PicturePart {
                attachment: Attachment {
                    kind: AttachmentKind::Pdf,
                    bytes: p.bytes,
                    name: format!("{} ({label})", a.name),
                },
                label,
                pages: own,
            }
        })
        .collect())
}

fn pictures_step(part: &PicturePart) -> Step<PicturesOut> {
    let layers: Vec<String> = part
        .pages
        .iter()
        .map(|(n, t)| format!("--- page {n} ---\n{t}"))
        .collect();
    let user = format!(
        "Read {}. These attached pages have a text layer, given below page by page. Some pictures on them may \
print text that is NOT in that text layer: a creature's stat block, a table, a sidebar or a rules box \
shown as an image. Transcribe VERBATIM the text printed inside such pictures: copy the printed words \
one by one; do NOT summarise, shorten, paraphrase, correct, complete or invent anything; keep every \
number, die expression and name exactly as printed (a creature's name and each variant's own name as \
printed, one stat block per printed stat block, never merged). Leave out text that is already in the \
text layer, and pictures that print no text (art, maps, portraits, ornaments): they get no block. One \
block per picture: `page` (its page number), `after` (the last line of the text layer, copied exactly, \
that comes before the picture in reading order; empty when the picture comes first on its page) and \
`text` (the picture's text: a stat block's name as a Markdown `###` heading, each line of the stat \
block on its own line, table rows as `cell | cell | cell`). No pictures with text: an empty `blocks`.\n\n{}",
        part.label,
        fence("user_input", &layers.join("\n\n"))
    );
    Step::new(
        "import_transcribe_pictures",
        "copy the text printed inside pictures of a D&D source word for word",
        "Output only text printed in the pictures, exactly, word for word; never paraphrase, condense or invent.",
        user,
        json!({"type":"object","required":["blocks"],"properties":{"blocks":{"type":"array","items":{
            "type":"object","required":["page","text"],"properties":{
                "page":{"type":"integer"},"after":{"type":"string"},"text":{"type":"string"}}}}}}),
    )
}

/// Words a block is compared on (letters only, four or more).
fn checked_words(text: &str) -> Vec<String> {
    norm_words(text)
        .split(' ')
        .filter(|w| w.chars().count() >= 4 && w.chars().all(char::is_alphabetic))
        .map(str::to_string)
        .collect()
}

/// The block only repeats its page's text layer.
fn echoes(block: &str, layer: &str) -> bool {
    let words = checked_words(block);
    if words.len() < ECHO_MIN_WORDS {
        return false;
    }
    let hay = format!(" {} ", norm_words(layer));
    let found = words
        .iter()
        .filter(|w| hay.contains(&format!(" {w} ")))
        .count();
    found as f32 >= ECHO_SHARE * words.len() as f32
}

/// Copy the pictures of `part`. A failed call is reported, never fatal: the text layer stands.
pub(super) async fn transcribe_pictures(
    runner: &dyn VisionRunner,
    part: &PicturePart,
    opts: &RunOptions<'_>,
    out: &mut Ingested,
) -> Vec<Inset> {
    let attachments = [part.attachment.clone()];
    let adapter = Attaching {
        inner: runner,
        attachments: &attachments,
    };
    out.calls += 1;
    let reply = match run_task(&adapter, &pictures_step(part), opts).await {
        Ok(r) => r.output,
        Err(e) => {
            out.notes.push(format!(
                "{READING_NOTE}{} could not be read ({e}); text printed in them (such as a stat block) is not in the source",
                part.label
            ));
            return Vec::new();
        }
    };
    let first = part.pages.first().map_or(0, |(n, _)| *n);
    let mut paraphrased = false;
    let mut kept = Vec::new();
    for b in reply.blocks {
        let page = if part.pages.iter().any(|(n, _)| *n == b.page) {
            b.page
        } else {
            first
        };
        let (text, marked) = strip_paraphrase(b.text.trim());
        paraphrased |= marked;
        let layer = part
            .pages
            .iter()
            .find(|(n, _)| *n == page)
            .map_or("", |(_, t)| t.as_str());
        if text.trim().is_empty() || echoes(&text, layer) {
            continue;
        }
        kept.push(Inset {
            page,
            after: b.after,
            text: text.trim().to_string(),
        });
    }
    if paraphrased {
        out.notes.push(format!(
            "{READING_NOTE}{}: the transcription says it is not a word-for-word copy, so text taken from these pictures is not the printed text",
            part.label
        ));
    }
    kept
}

/// Byte offset of the start of the line holding `at`.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// Byte offset of the end of the line holding `at`.
fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i)
}

/// Where each page of `pages` (number, its own rendered text) starts in `text`: the line of
/// its first printed line, searched after the previous page's start.
fn page_starts(text: &str, pages: &[(u32, String)]) -> Vec<(u32, usize)> {
    let mut from = 0;
    let mut out = Vec::new();
    for (n, own) in pages {
        let first = own
            .lines()
            .map(|l| l.trim().trim_start_matches('#').trim())
            .find(|l| !l.is_empty());
        if let Some(at) = first.and_then(|f| text[from..].find(f)) {
            from = line_start(text, from + at);
        }
        out.push((*n, from));
    }
    out
}

/// `prefix` cut to at most `max` characters.
fn prefix(s: &str, max: usize) -> &str {
    s.char_indices().nth(max).map_or(s, |(i, _)| &s[..i])
}

/// Where the block of `inset` goes in `text[start..end]` (the page): after the line holding
/// its anchor, else at the end of the page.
fn insertion(text: &str, start: usize, end: usize, inset: &Inset) -> usize {
    let anchor = inset.after.trim().trim_start_matches('#').trim();
    let page = &text[start..end];
    let found = [anchor, prefix(anchor, ANCHOR_PREFIX_CHARS)]
        .into_iter()
        .filter(|a| !a.is_empty())
        .find_map(|a| page.find(a).map(|i| start + i + a.len()));
    match found {
        Some(at) => line_end(text, at),
        None if anchor.is_empty() => start,
        None => end,
    }
}

/// `block` with its leading heading made a bold line when it repeats the heading the block
/// sits under (a creature's lore heading above its stat block picture): the stat block then
/// stays in that section instead of opening a second one with the same name.
fn under_heading(block: &str, before: &str) -> String {
    let Some(first) = block.lines().next() else {
        return block.to_string();
    };
    let Some(name) = first
        .strip_prefix('#')
        .map(|n| n.trim_start_matches('#').trim())
    else {
        return block.to_string();
    };
    let above = before
        .lines()
        .rev()
        .find(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim());
    if above.is_some_and(|a| norm_words(a) == norm_words(name)) {
        let rest: Vec<&str> = block.lines().skip(1).collect();
        std::iter::once(format!("**{name}**"))
            .chain(rest.into_iter().map(str::to_string))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        block.to_string()
    }
}

/// `text` (the rendered text of `pages`) with each inset placed on its page after the line it
/// follows (at the end of the page when that line is not found).
pub(super) fn splice(text: &str, pages: &[(u32, String)], insets: &[Inset]) -> String {
    let starts = page_starts(text, pages);
    let span = |page: u32| -> (usize, usize) {
        let k = starts.iter().position(|(n, _)| *n == page).unwrap_or(0);
        let start = starts.get(k).map_or(0, |s| s.1);
        let end = starts
            .iter()
            .skip(k + 1)
            .map(|s| s.1)
            .find(|e| *e > start)
            .unwrap_or(text.len());
        (start, end)
    };
    let mut placed: Vec<(usize, usize)> = insets
        .iter()
        .enumerate()
        .map(|(i, inset)| {
            let (start, end) = span(inset.page);
            (insertion(text, start, end, inset), i)
        })
        .collect();
    placed.sort();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (pos, i) in placed {
        out.push_str(&text[at..pos]);
        let block = under_heading(&insets[i].text, &out);
        let head = if out.is_empty() || out.ends_with("\n\n") {
            ""
        } else if out.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        out.push_str(head);
        out.push_str(&block);
        // A blank line before what follows, counting the newlines it starts with.
        let follows = text[pos..]
            .chars()
            .take(2)
            .take_while(|c| *c == '\n')
            .count();
        out.push_str(&"\n".repeat(2 - follows));
        at = pos;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
#[path = "pictures_tests.rs"]
mod tests;
