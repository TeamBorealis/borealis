//! Pure-Rust PDF text extraction (no OCR) with layout cues: text runs are placed by the text and
//! graphics matrices, grouped into lines, two-column pages are read column by column, running
//! headers/footers are dropped, and font sizes / bold faces become heading levels.

use std::collections::BTreeMap;

use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};

use super::layout::{render, HeadingScale, TextLine};
use super::pdf_fonts::{page_fonts, shown, Spacing};
use crate::error::AssistError;

/// A PDF with fewer non-whitespace characters per page than this is treated as scanned images.
pub const MIN_CHARS_PER_PAGE: usize = 20;

/// Stack for the PDF parsing thread.
const PDF_THREAD_STACK: usize = 64 * 1024 * 1024;
/// Stop collecting text once this many bytes are extracted (flate-bomb guard).
pub const MAX_EXTRACT_BYTES: usize = 8 * 1024 * 1024;
/// Most decompressed content read from one page (flate-bomb guard).
const MAX_PAGE_CONTENT: usize = 32 * 1024 * 1024;
/// Latin letters a page needs before its vowel share is judged.
const GARBLE_MIN_LETTERS: usize = 200;
/// Below this share of vowels among Latin letters the text layer is nonsense (prose in
/// European languages has about 35-45%).
const GARBLE_VOWEL_SHARE: f32 = 0.2;
/// Share of a font's characters one symbol must reach to be that font's word space.
const SPACE_STAND_IN_SHARE: f32 = 0.08;
/// Symbols some fonts decode their space glyph to.
const SPACE_STAND_INS: &str = "%!$#&*+=@^_|~";
/// Page width when the page declares none (US letter).
const DEFAULT_WIDTH: f32 = 612.0;
/// Runs whose baselines differ by less than this share of the font size are one line.
const SAME_LINE: f32 = 0.35;
/// A horizontal gap wider than this many font sizes splits a line into fragments.
const SPLIT_GAP: f32 = 1.5;
/// A gap wider than this share of the font size between two runs is a word space.
const WORD_GAP: f32 = 0.15;
/// Fragments on each side of the middle before a page counts as two columns.
const MIN_COLUMN_LINES: usize = 3;
/// A vertical gap of more than this many line heights is a paragraph break.
const PARA_GAP: f32 = 1.6;

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
}

impl PdfPage {
    /// Visible characters on the page.
    pub fn chars(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.text.chars().filter(|c| !c.is_whitespace()).count())
            .sum()
    }

    /// The page has a usable text layer.
    pub fn has_text(&self) -> bool {
        self.chars() >= MIN_CHARS_PER_PAGE && !self.garbled()
    }

    /// The text layer decodes to nonsense (a font without a usable character map): too few
    /// vowels among Latin letters. Such a page is read visually instead.
    pub fn garbled(&self) -> bool {
        let letters: Vec<char> = self
            .lines
            .iter()
            .flat_map(|l| l.text.chars())
            .filter(char::is_ascii_alphabetic)
            .collect();
        let vowels = letters
            .iter()
            .filter(|c| "aeiouyAEIOUY".contains(**c))
            .count();
        letters.len() >= GARBLE_MIN_LETTERS
            && (vowels as f32) < GARBLE_VOWEL_SHARE * letters.len() as f32
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

// ---- positioned runs ---------------------------------------------------------------------------

type Matrix = [f32; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn translate(tx: f32, ty: f32) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

#[derive(Debug, Clone)]
struct Run {
    x: f32,
    y: f32,
    end: f32,
    size: f32,
    bold: bool,
    text: String,
    font: Vec<u8>,
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    font: Vec<u8>,
    size: f32,
    leading: f32,
    char_space: f32,
    word_space: f32,
    scale: f32,
}

fn num(o: &Object) -> f32 {
    o.as_float().unwrap_or(0.0)
}

fn nums<const N: usize>(ops: &[Object]) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = num(ops.get(i)?);
    }
    Some(out)
}

fn page_runs(doc: &Document, page: ObjectId, budget: &mut usize) -> Vec<Run> {
    let Ok(data) = doc.get_page_content_with_limit(page, MAX_PAGE_CONTENT) else {
        return Vec::new();
    };
    let Ok(content) = Content::decode(&data) else {
        return Vec::new();
    };
    let fonts = page_fonts(doc, page);
    let mut stack: Vec<State> = Vec::new();
    let mut st = State {
        ctm: IDENTITY,
        font: Vec::new(),
        size: 0.0,
        leading: 0.0,
        char_space: 0.0,
        word_space: 0.0,
        scale: 1.0,
    };
    let (mut tm, mut lm) = (IDENTITY, IDENTITY);
    let mut runs = Vec::new();
    for op in &content.operations {
        let o = &op.operands;
        match op.operator.as_str() {
            "q" => stack.push(st.clone()),
            "Q" => st = stack.pop().unwrap_or(st),
            "cm" => {
                if let Some(m) = nums::<6>(o) {
                    st.ctm = mul(&m, &st.ctm);
                }
            }
            "BT" => (tm, lm) = (IDENTITY, IDENTITY),
            "Tf" => {
                st.font = o
                    .first()
                    .and_then(|f| f.as_name().ok())
                    .unwrap_or_default()
                    .to_vec();
                st.size = o.get(1).map(num).unwrap_or(0.0);
            }
            "TL" => st.leading = o.first().map(num).unwrap_or(0.0),
            "Tc" => st.char_space = o.first().map(num).unwrap_or(0.0),
            "Tw" => st.word_space = o.first().map(num).unwrap_or(0.0),
            "Tz" => st.scale = o.first().map_or(100.0, num) / 100.0,
            "Td" | "TD" => {
                if let Some([tx, ty]) = nums::<2>(o) {
                    if op.operator == "TD" {
                        st.leading = -ty;
                    }
                    lm = mul(&translate(tx, ty), &lm);
                    tm = lm;
                }
            }
            "Tm" => {
                if let Some(m) = nums::<6>(o) {
                    (tm, lm) = (m, m);
                }
            }
            "T*" => {
                lm = mul(&translate(0.0, -st.leading), &lm);
                tm = lm;
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if op.operator != "Tj" && op.operator != "TJ" {
                    lm = mul(&translate(0.0, -st.leading), &lm);
                    tm = lm;
                }
                let strings = if op.operator == "\"" {
                    o.get(2..).unwrap_or_default()
                } else {
                    o
                };
                let font = fonts.get(&st.font);
                let sp = Spacing {
                    size: st.size,
                    char_space: st.char_space,
                    word_space: st.word_space,
                    scale: st.scale,
                };
                let (text, advance) = shown(font, strings, &sp);
                let m = mul(&tm, &st.ctm);
                let scale = (m[2] * m[2] + m[3] * m[3]).sqrt();
                let size = (st.size * scale).abs();
                let width = advance * (m[0] * m[0] + m[1] * m[1]).sqrt();
                tm = mul(&translate(advance, 0.0), &tm);
                if text.trim().is_empty() || *budget == 0 {
                    continue;
                }
                *budget = budget.saturating_sub(text.len());
                runs.push(Run {
                    x: m[4],
                    y: m[5],
                    end: m[4] + width,
                    size,
                    bold: font.is_some_and(|f| f.bold),
                    text,
                    font: st.font.clone(),
                });
            }
            _ => {}
        }
    }
    runs
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
}

// ---- lines and columns -------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Fragment {
    x: f32,
    end: f32,
    y: f32,
    size: f32,
    bold: bool,
    text: String,
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

fn fragments(mut runs: Vec<Run>) -> Vec<Fragment> {
    runs.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    // Baselines first (runs within SAME_LINE of the line's first run), then left to right.
    let mut lines: Vec<Vec<Run>> = Vec::new();
    for r in runs {
        match lines.last_mut() {
            Some(l) if (l[0].y - r.y).abs() <= SAME_LINE * l[0].size.max(r.size).max(1.0) => {
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
                if gap <= SPLIT_GAP * em {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Full,
}

/// Reading order: one column top to bottom, or, on a two-column page, each band between
/// full-width lines read left column then right column.
fn reading_order(frags: Vec<Fragment>, width: f32) -> Vec<Fragment> {
    let mid = width / 2.0;
    let slack = width * 0.04;
    let side = |f: &Fragment| {
        if f.end <= mid + slack {
            Side::Left
        } else if f.x >= mid - slack {
            Side::Right
        } else {
            Side::Full
        }
    };
    let lefts = frags.iter().filter(|f| side(f) == Side::Left).count();
    let rights = frags.iter().filter(|f| side(f) == Side::Right).count();
    if lefts < MIN_COLUMN_LINES || rights < MIN_COLUMN_LINES {
        return merge_rows(frags);
    }
    let mut out = Vec::new();
    let (mut left, mut right): (Vec<Fragment>, Vec<Fragment>) = (Vec::new(), Vec::new());
    for f in frags {
        match side(&f) {
            Side::Left => left.push(f),
            Side::Right => right.push(f),
            Side::Full => {
                out.append(&mut left);
                out.append(&mut right);
                out.push(f);
            }
        }
    }
    out.append(&mut left);
    out.append(&mut right);
    out
}

/// Single-column page: fragments on one baseline (table cells) become one `a | b` line.
fn merge_rows(frags: Vec<Fragment>) -> Vec<Fragment> {
    let mut out: Vec<Fragment> = Vec::new();
    for f in frags {
        match out.last_mut() {
            Some(prev) if (prev.y - f.y).abs() <= SAME_LINE * prev.size.max(f.size).max(1.0) => {
                prev.text = format!("{} | {}", prev.text.trim_end(), f.text.trim_start());
                prev.end = f.end;
                prev.bold &= f.bold;
            }
            _ => out.push(f),
        }
    }
    out
}

fn page_width(doc: &Document, page: ObjectId) -> f32 {
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
                if w > 0.0 {
                    return w;
                }
            }
        }
        id = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    DEFAULT_WIDTH
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
    let mut pages: Vec<PdfPage> = doc
        .get_pages()
        .into_iter()
        .map(|(number, id)| {
            let mut runs = page_runs(doc, id, &mut budget);
            restore_spaces(&mut runs);
            let ordered = reading_order(fragments(runs), page_width(doc, id));
            PdfPage {
                number,
                lines: to_lines(&ordered),
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
