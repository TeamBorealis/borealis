//! Pure-Rust PDF text extraction (no OCR) with layout cues: text runs are placed by the text and
//! graphics matrices, grouped into lines, two-column pages are read column by column, running
//! headers/footers are dropped, and font sizes / bold faces become heading levels.

use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};

use super::layout::{render, HeadingScale, TextLine};
use super::pdf_fonts::FontCache;
use super::pdf_trust::{distrust, Trust};
use super::pdf_walk::{paint_page, Run};
use crate::error::AssistError;

// A child of this module so the import module list stays as it is.
#[path = "pdf_columns.rs"]
mod columns;
use columns::reading_order;

/// A PDF with fewer non-whitespace characters per page than this is treated as scanned images.
pub const MIN_CHARS_PER_PAGE: usize = 20;

/// Stack for the PDF parsing thread.
const PDF_THREAD_STACK: usize = 64 * 1024 * 1024;
/// Stop collecting text once this many bytes are extracted (flate-bomb guard).
pub const MAX_EXTRACT_BYTES: usize = 8 * 1024 * 1024;
/// Share of a font's characters one symbol must reach to be that font's word space.
const SPACE_STAND_IN_SHARE: f32 = 0.08;
/// Symbols some fonts decode their space glyph to.
const SPACE_STAND_INS: &str = "%!$#&*+=@^_|~";
/// Page size when the page declares none (US letter).
const DEFAULT_SIZE: (f32, f32) = (612.0, 792.0);
/// Runs whose baselines differ by less than this share of the font size are one line.
const SAME_LINE: f32 = 0.35;
/// A horizontal gap wider than this many font sizes splits a line into fragments.
const SPLIT_GAP: f32 = 1.5;
/// A gap wider than this share of the font size between two runs is a word space.
const WORD_GAP: f32 = 0.15;
/// A vertical gap of more than this many line heights is a paragraph break.
const PARA_GAP: f32 = 1.6;
/// Runs of the same text closer than this many font sizes are one overprinted run.
const OVERPRINT: f32 = 0.3;
/// A run this much smaller than its neighbour may sit this far off the baseline (superscript
/// "th" of "4th", a footnote mark) and still belong to the line.
const SUPERSCRIPT_SIZE: f32 = 0.85;
const SUPERSCRIPT_RISE: f32 = 0.6;

/// Text pulled out of a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfText {
    /// Extracted text, headings as `#` lines.
    pub text: String,
    /// Page count.
    pub pages: usize,
}

/// One page's lines in reading order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PdfPage {
    /// 1-based page number.
    pub number: u32,
    /// Lines in reading order.
    pub lines: Vec<TextLine>,
    /// Why the text layer cannot stand for the page (it decodes to nonsense, or the page
    /// shows text that is not in it); `None` when it can.
    pub distrust: Option<String>,
}

impl PdfPage {
    /// Visible characters on the page.
    pub fn chars(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.text.chars().filter(|c| !c.is_whitespace()).count())
            .sum()
    }

    /// The page has a usable text layer: enough text, and nothing says it is garbled or
    /// incomplete. Other pages are read visually.
    pub fn has_text(&self) -> bool {
        self.chars() >= MIN_CHARS_PER_PAGE && self.distrust.is_none()
    }

    /// The text layer decodes to nonsense (a font without a usable character map).
    pub fn garbled(&self) -> bool {
        matches!(super::pdf_trust::judge_text(&self.lines), Trust::Garbled(_))
    }
}

/// Every page of a PDF with its text layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PdfLayout {
    /// Pages in order.
    pub pages: Vec<PdfPage>,
}

impl PdfLayout {
    /// The heading scale of the whole document.
    pub fn scale(&self) -> HeadingScale {
        HeadingScale::of(self.pages.iter().flat_map(|p| p.lines.iter()))
    }

    /// Text of `pages` (indices into [`Self::pages`]) with the document's heading scale.
    pub fn text_of(&self, pages: &[usize], scale: &HeadingScale) -> String {
        let lines: Vec<TextLine> = pages
            .iter()
            .filter_map(|&i| self.pages.get(i))
            .flat_map(|p| p.lines.iter().cloned())
            .collect();
        render(&lines, scale)
    }

    /// The whole document's text.
    pub fn text(&self) -> String {
        let all: Vec<usize> = (0..self.pages.len()).collect();
        self.text_of(&all, &self.scale())
    }
}

fn on_big_stack<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AssistError> + Send + 'static,
) -> Result<T, AssistError> {
    // lopdf recurses on nested objects; run on a big-stack thread so a hostile file yields an
    // error instead of aborting the process.
    std::thread::Builder::new()
        .name("pdf-extract".into())
        .stack_size(PDF_THREAD_STACK)
        .spawn(f)
        .map_err(|e| AssistError::Extraction(format!("could not start PDF reader: {e}")))?
        .join()
        .map_err(|_| {
            AssistError::Extraction("the PDF could not be processed (parser panicked)".into())
        })?
}

/// Load a PDF (on a big-stack thread).
///
/// # Errors
/// [`AssistError::Extraction`] when the file cannot be parsed.
pub fn load_pdf(bytes: &[u8]) -> Result<Document, AssistError> {
    let owned = bytes.to_vec();
    on_big_stack(move || {
        Document::load_mem(&owned)
            .map_err(|e| AssistError::Extraction(format!("the PDF could not be read: {e}")))
    })
}

/// The text layer of every page, with layout cues.
///
/// # Errors
/// [`AssistError::Extraction`] when the file cannot be parsed.
pub fn read_pdf_layout(bytes: &[u8]) -> Result<PdfLayout, AssistError> {
    let owned = bytes.to_vec();
    on_big_stack(move || {
        let doc = Document::load_mem(&owned)
            .map_err(|e| AssistError::Extraction(format!("the PDF could not be read: {e}")))?;
        Ok(layout_of(&doc))
    })
}

/// Extract the text layer of a PDF.
///
/// # Errors
/// [`AssistError::Extraction`] when the file cannot be parsed, or yields too little text (a scanned
/// PDF); Borealis does not run OCR, so the message tells the user to paste the text instead.
pub fn extract_pdf_text(bytes: &[u8]) -> Result<PdfText, AssistError> {
    let layout = read_pdf_layout(bytes)?;
    let pages = layout.pages.len();
    let chars: usize = layout.pages.iter().map(PdfPage::chars).sum();
    if pages == 0 || chars < MIN_CHARS_PER_PAGE * pages {
        return Err(AssistError::Extraction(format!(
            "the PDF has almost no text layer ({chars} characters over {pages} page(s)); it is probably scanned images. \
Borealis does not run OCR: paste the text instead, or run OCR in another tool first."
        )));
    }
    Ok(PdfText {
        text: layout.text(),
        pages,
    })
}

// ---- runs ---------------------------------------------------------------------------------

fn num(o: &Object) -> f32 {
    o.as_float().unwrap_or(0.0)
}

/// Overprinted text (fake bold drawn twice, a drop shadow) is one run: a run whose text an
/// earlier run already shows at (almost) the same place is dropped.
fn drop_overprints(runs: Vec<Run>) -> Vec<Run> {
    let mut kept: Vec<Run> = Vec::with_capacity(runs.len());
    let mut by_text: std::collections::HashMap<String, Vec<usize>> = Default::default();
    for r in runs {
        let key = r.text.trim().to_string();
        // Closer than a share of the run's own width: a repeated letter drawn on its own
        // ("l" then "l") sits a full letter further on.
        let near = |k: &Run| {
            let em = k.size.max(r.size).max(1.0);
            let reach = (OVERPRINT * em).min(OVERPRINT * (k.end - k.x).abs());
            (k.x - r.x).abs() < reach && (k.y - r.y).abs() < OVERPRINT * em
        };
        let seen = by_text
            .get(&key)
            .is_some_and(|ix| ix.iter().any(|&i| near(&kept[i])));
        if seen {
            continue;
        }
        by_text.entry(key).or_default().push(kept.len());
        kept.push(r);
    }
    kept
}

/// `Word%Word%Word%` in one run (no spaces, a stand-in symbol between words and at the end):
/// the font decodes its space to that symbol. Short runs (headings) the per-font repair misses.
fn short_run_spaces(text: &str) -> Option<String> {
    let t = text.trim();
    if t.contains(' ') || t.chars().count() < 3 {
        return None;
    }
    let last = t.chars().last()?;
    if !(SPACE_STAND_INS.contains(last) || last == '\'') {
        return None;
    }
    // A single word with a stand-in after it ("Items%"): a percent sign never follows letters.
    let body = &t[..t.len() - last.len_utf8()];
    if last == '%' && !body.contains(last) && body.chars().all(char::is_alphabetic) {
        return Some(body.to_string());
    }
    let parts: Vec<&str> = t[..t.len() - last.len_utf8()].split(last).collect();
    let words = parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(char::is_alphanumeric));
    words.then(|| parts.join(" "))
}

/// Some fonts decode their space glyph to a symbol ("word%word%word"): when a font's text has
/// almost no spaces and one symbol makes up a large share of it, that symbol is its space.
fn restore_spaces(runs: &mut [Run]) {
    let mut fonts: BTreeMap<Vec<u8>, String> = BTreeMap::new();
    for r in runs.iter() {
        fonts.entry(r.font.clone()).or_default().push_str(&r.text);
    }
    for (font, text) in fonts {
        let total = text.chars().count();
        let spaces = text.chars().filter(|c| *c == ' ').count();
        if total < 40 || spaces * 50 > total {
            continue;
        }
        let stand_in = SPACE_STAND_INS
            .chars()
            .max_by_key(|s| text.matches(*s).count());
        let Some(c) = stand_in
            .filter(|c| text.matches(*c).count() as f32 >= SPACE_STAND_IN_SHARE * total as f32)
        else {
            continue;
        };
        for r in runs.iter_mut().filter(|r| r.font == font) {
            r.text = r.text.replace(c, " ");
        }
    }
    for r in runs.iter_mut() {
        if let Some(fixed) = short_run_spaces(&r.text) {
            r.text = fixed;
        }
    }
}

// ---- lines and columns -------------------------------------------------------------------------

/// Text on one baseline between two wide gaps (a line of a column, a table cell line).
#[derive(Debug, Clone)]
pub(crate) struct Fragment {
    pub x: f32,
    pub end: f32,
    pub y: f32,
    pub size: f32,
    pub bold: bool,
    pub text: String,
}

/// A fragment being built: its runs' characters by size and how many are bold.
struct Building {
    frag: Fragment,
    by_size: Vec<(f32, usize)>,
    chars: usize,
    bold: usize,
}

impl Building {
    fn new(r: Run) -> Self {
        let mut b = Self {
            frag: Fragment {
                x: r.x,
                end: r.end,
                y: r.y,
                size: r.size,
                bold: false,
                text: String::new(),
            },
            by_size: Vec::new(),
            chars: 0,
            bold: 0,
        };
        b.add(r);
        b
    }

    fn add(&mut self, r: Run) {
        let n = r.text.chars().filter(|c| !c.is_whitespace()).count();
        match self
            .by_size
            .iter_mut()
            .find(|(s, _)| (s - r.size).abs() < 0.3)
        {
            Some(e) => e.1 += n,
            None => self.by_size.push((r.size, n)),
        }
        self.chars += n;
        self.bold += if r.bold { n } else { 0 };
        self.frag.end = self.frag.end.max(r.end);
        self.frag.text.push_str(&r.text);
    }

    /// The fragment, sized by the size most of its characters have (a drop cap or a larger
    /// symbol does not make a line a heading).
    fn done(self) -> Fragment {
        let size = self
            .by_size
            .iter()
            .max_by_key(|(_, n)| *n)
            .map_or(self.frag.size, |(s, _)| *s);
        Fragment {
            size,
            bold: self.chars > 0 && self.bold == self.chars,
            ..self.frag
        }
    }
}

fn same_line(a: &Run, b: &Run) -> bool {
    (a.y - b.y).abs() <= SAME_LINE * a.size.max(b.size).max(1.0)
}

/// `small` is a raised or lowered smaller run touching `big` (an ordinal suffix, a mark).
fn superscript_of(a: &Run, b: &Run) -> bool {
    let (big, small) = if a.size >= b.size { (a, b) } else { (b, a) };
    let em = big.size.max(1.0);
    small.size < SUPERSCRIPT_SIZE * big.size
        && (big.y - small.y).abs() <= SUPERSCRIPT_RISE * em
        && (small.x - big.end).abs().min((big.x - small.end).abs()) <= WORD_GAP * em
}

fn fragments(mut runs: Vec<Run>) -> Vec<Fragment> {
    runs.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let stops = super::pdf_table::tab_stops(&runs);
    // Baselines first (runs within SAME_LINE of the line's first run), then left to right.
    let mut lines: Vec<Vec<Run>> = Vec::new();
    for r in runs {
        match lines.last_mut() {
            Some(l) if l.iter().any(|k| same_line(k, &r) || superscript_of(k, &r)) => {
                l.push(r);
            }
            _ => lines.push(vec![r]),
        }
    }
    let mut out = Vec::new();
    for mut line in lines {
        line.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut cur: Option<Building> = None;
        for r in line {
            if let Some(b) = cur.as_mut() {
                let gap = r.x - b.frag.end;
                let em = b.frag.size.max(r.size);
                let cell = gap >= super::pdf_table::CELL_GAP * em
                    && super::pdf_table::at_stop(&stops, r.x)
                    && (b.frag.x - r.x).abs() > 1.0;
                if gap <= SPLIT_GAP * em && !cell {
                    if gap > WORD_GAP * em
                        && !b.frag.text.ends_with(' ')
                        && !r.text.starts_with(' ')
                    {
                        b.frag.text.push(' ');
                    }
                    b.add(r);
                    continue;
                }
                out.extend(cur.take().map(Building::done));
            }
            cur = Some(Building::new(r));
        }
        out.extend(cur.map(Building::done));
    }
    out
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
            if b.len() == 4 {
                let w = (num(&b[2]) - num(&b[0])).abs();
                let h = (num(&b[3]) - num(&b[1])).abs();
                if w > 0.0 && h > 0.0 {
                    return (w, h);
                }
            }
        }
        id = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    DEFAULT_SIZE
}

fn to_lines(frags: &[Fragment]) -> Vec<TextLine> {
    let mut out = Vec::new();
    let mut prev: Option<&Fragment> = None;
    for f in frags {
        let gap = prev.is_some_and(|p| {
            let dy = p.y - f.y;
            dy < 0.0 || dy > PARA_GAP * p.size.max(f.size).max(1.0)
        });
        out.push(TextLine {
            text: f.text.split_whitespace().collect::<Vec<_>>().join(" "),
            size: f.size,
            bold: f.bold,
            level: None,
            gap_before: gap,
        });
        prev = Some(f);
    }
    out
}

/// Header/footer key: digits folded so running page numbers compare equal.
fn edge_key(t: &str) -> String {
    t.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                '#'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_page_number(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty() && t.len() <= 4 && t.chars().all(|c| c.is_ascii_digit())
}

/// Drop running headers/footers: the first or last line of a page that repeats (digits folded)
/// at the edge of at least half the pages, and bare page numbers at the edges.
fn strip_running_lines(pages: &mut [PdfPage]) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for p in pages.iter() {
        let mut keys: Vec<String> = p
            .lines
            .first()
            .into_iter()
            .chain(p.lines.last())
            .map(|l| edge_key(&l.text))
            .collect();
        keys.dedup();
        for k in keys {
            *seen.entry(k).or_default() += 1;
        }
    }
    let needed = (pages.len() / 2).max(3);
    let repeated = |t: &str| seen.get(&edge_key(t)).is_some_and(|n| *n >= needed);
    for p in pages.iter_mut() {
        for edge in [0, 1] {
            let pick = |lines: &Vec<TextLine>| {
                if edge == 0 {
                    lines.first().cloned()
                } else {
                    lines.last().cloned()
                }
            };
            if let Some(l) = pick(&p.lines) {
                if repeated(&l.text) || is_page_number(&l.text) {
                    if edge == 0 {
                        p.lines.remove(0);
                    } else {
                        p.lines.pop();
                    }
                }
            }
        }
    }
}

fn layout_of(doc: &Document) -> PdfLayout {
    let mut budget = MAX_EXTRACT_BYTES;
    let mut cache = FontCache::default();
    let mut pages: Vec<PdfPage> = doc
        .get_pages()
        .into_iter()
        .map(|(number, id)| {
            let painted = paint_page(doc, id, &mut cache, &mut budget);
            let mut runs = drop_overprints(painted.runs);
            restore_spaces(&mut runs);
            let (width, height) = page_size(doc, id);
            let ordered = reading_order(fragments(runs), width);
            let lines = to_lines(&ordered);
            let distrust = distrust(&lines, &painted.images, (width, height));
            PdfPage {
                number,
                lines,
                distrust,
            }
        })
        .collect();
    strip_running_lines(&mut pages);
    PdfLayout { pages }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::testing::synthetic_pdf as build;

    #[test]
    fn two_columns_read_column_by_column_with_headings() {
        // Lines of both columns are interleaved in the content stream (row by row).
        let mut page = vec![(72.0, 740.0, 20.0, true, "Lantern Hollow")];
        for i in 0..6 {
            let y = 700.0 - 14.0 * i as f32;
            page.push((
                72.0,
                y,
                10.0,
                false,
                [
                    "left one",
                    "left two",
                    "left three",
                    "left four",
                    "left five",
                    "left six",
                ][i],
            ));
            page.push((
                330.0,
                y,
                10.0,
                false,
                [
                    "right one",
                    "right two",
                    "right three",
                    "right four",
                    "right five",
                    "right six",
                ][i],
            ));
        }
        page.push((
            72.0,
            600.0,
            10.0,
            false,
            "Below both columns there is a closing line that spans the page width.",
        ));
        let pdf = build(&[page], 0);
        let t = extract_pdf_text(&pdf).unwrap().text;
        assert!(t.starts_with("# Lantern Hollow"), "{t}");
        let order: Vec<usize> = [
            "left one",
            "left six",
            "right one",
            "right six",
            "Below both",
        ]
        .iter()
        .map(|s| t.find(s).unwrap_or(usize::MAX))
        .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{t}");
    }

    /// Offline look at one page: `BOREALIS_PDF_FRAGS=<file.pdf>:<page>` prints its fragments
    /// (x, end, y, size, text) in reading order.
    #[test]
    #[ignore = "needs a local PDF"]
    fn probe_page_fragments() {
        let Ok(arg) = std::env::var("BOREALIS_PDF_FRAGS") else {
            return;
        };
        let (file, page) = arg.rsplit_once(':').unwrap();
        let doc = load_pdf(&std::fs::read(file).unwrap()).unwrap();
        let id = doc.get_pages()[&page.parse::<u32>().unwrap()];
        let (mut cache, mut budget) = (FontCache::default(), MAX_EXTRACT_BYTES);
        let painted = paint_page(&doc, id, &mut cache, &mut budget);
        let mut runs = drop_overprints(painted.runs);
        restore_spaces(&mut runs);
        let (width, _) = page_size(&doc, id);
        let raw = std::env::var("BOREALIS_PDF_RAW").is_ok();
        let frags = fragments(runs);
        let frags = if raw {
            frags
        } else {
            reading_order(frags, width)
        };
        for f in frags {
            println!(
                "{:6.1} {:6.1} {:6.1} {:4.1} {}",
                f.x, f.end, f.y, f.size, f.text
            );
        }
    }

    #[test]
    fn running_headers_and_page_numbers_are_dropped() {
        let pages: Vec<Vec<_>> = (1..=4)
            .map(|n| {
                vec![
                    (72.0, 760.0, 8.0, false, "THE TINKER'S ALMANAC"),
                    (
                        72.0,
                        700.0,
                        10.0,
                        false,
                        "Body text of a page with enough characters.",
                    ),
                    (300.0, 30.0, 8.0, false, ["1", "2", "3", "4"][n - 1]),
                ]
            })
            .collect();
        let t = extract_pdf_text(&build(&pages, 0)).unwrap().text;
        assert!(!t.contains("ALMANAC"), "{t}");
        assert!(!t.lines().any(|l| l.trim() == "3"), "{t}");
        assert_eq!(t.matches("Body text").count(), 4);
    }
}
