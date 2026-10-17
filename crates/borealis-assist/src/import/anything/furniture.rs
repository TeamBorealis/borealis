//! Page furniture of a printed source: page numbers, running headers ("<TITLE> | 12"), a
//! repeated footer sentence ("Not for sale ...") or web address, and extraction debris (a line
//! of quote marks). It is printed on every page, never part of an element's text: it is left
//! out of the text built from source passages and out of the descriptions.

use std::collections::{HashMap, HashSet};

/// Fewest occurrences of a line shape (digits counted as one) that make a running header.
const MIN_HEADER_REPEATS: usize = 3;
/// Fewest identical occurrences of a sentence that make it a footer.
const MIN_FOOTER_REPEATS: usize = 5;
/// Fewest lines between two printings of a footer (one per page, never a run of them).
const MIN_PAGE_GAP: usize = 10;
/// Fewest words of a footer sentence.
const MIN_FOOTER_WORDS: usize = 6;
/// Most words of a web address line ("More at example.com").
const MAX_WEB_WORDS: usize = 8;
/// Most digits of a page number.
const MAX_PAGE_DIGITS: usize = 4;
/// Fewest letters of a running title.
const MIN_TITLE_LETTERS: usize = 4;

/// The page furniture of one source: its line numbers (0-based) and its distinct texts.
#[derive(Debug, Default, Clone)]
pub(crate) struct Furniture {
    lines: HashSet<usize>,
    texts: Vec<String>,
}

impl Furniture {
    /// The line at `i` (0-based) is page furniture.
    pub(crate) fn has(&self, i: usize) -> bool {
        self.lines.contains(&i)
    }

    /// The furniture texts worth removing from a written description (never a bare page
    /// number, which a description prints as a value).
    pub(crate) fn texts(&self) -> &[String] {
        &self.texts
    }
}

/// A page number alone ("12"), or one an extraction marked ("12!", "!").
fn page_number(t: &str) -> bool {
    let digits = t.chars().filter(char::is_ascii_digit).count();
    !t.is_empty() && digits <= MAX_PAGE_DIGITS && t.chars().all(|c| c.is_ascii_digit() || c == '!')
}

fn marked_page_number(t: &str) -> bool {
    page_number(t) && t.contains('!')
}

fn cells(t: &str) -> Vec<&str> {
    t.split(" | ").map(str::trim).collect()
}

fn letters(t: &str) -> Vec<char> {
    t.chars().filter(|c| c.is_alphabetic()).collect()
}

fn all_caps(t: &str) -> bool {
    let l = letters(t);
    l.len() >= MIN_TITLE_LETTERS && l.iter().all(|c| c.is_uppercase())
}

/// A running header: a title with its page number after it ("<Title> | 12!", "<TITLE> | 12"), a
/// title in capitals with its page number ("12 <TITLE>"), or titles in capitals ("<BOOK> |
/// <CHAPTER>").
fn header_shape(t: &str) -> bool {
    let c = cells(t);
    if let [rest @ .., last] = c.as_slice() {
        if !rest.is_empty() {
            // The page number closes the line (a roll table's number opens its rows); a plain one
            // only after titles without numbers ("Level | 2" is a table row).
            let numbered = marked_page_number(last)
                || (page_number(last)
                    && !rest.iter().any(|x| x.chars().any(|ch| ch.is_ascii_digit())));
            let titled = rest.iter().any(|x| letters(x).len() >= MIN_TITLE_LETTERS);
            return (numbered && titled) || c.iter().all(|x| all_caps(x) || page_number(x));
        }
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    let (first, last) = (words.first().copied(), words.last().copied());
    words.len() >= 2
        && (first.is_some_and(page_number) || last.is_some_and(page_number))
        && all_caps(t)
}

/// A line naming a web address ("www.example.com", "example.blog").
fn web_line(t: &str) -> bool {
    let words: Vec<&str> = t.split_whitespace().collect();
    words.len() <= MAX_WEB_WORDS
        && !t.contains(" | ")
        && words.iter().any(|w| {
            let w = w.trim_end_matches(['.', ',', ')']);
            w.starts_with("http") || w.starts_with("www.") || {
                let parts: Vec<&str> = w.split('.').collect();
                parts.len() >= 2
                    && parts
                        .iter()
                        .all(|p| p.len() >= 2 && p.chars().all(char::is_alphanumeric))
                    && parts
                        .last()
                        .is_some_and(|p| p.chars().all(|c| c.is_ascii_lowercase()))
            }
        })
}

/// Extraction debris: a line of quote marks and spaces only.
fn debris(t: &str) -> bool {
    !t.is_empty()
        && t.chars()
            .all(|c| matches!(c, '\'' | '’' | '‘' | '"' | '`' | ' '))
}

fn shape(t: &str) -> String {
    let mut out = String::new();
    for c in t.trim().to_lowercase().chars() {
        if c.is_ascii_digit() {
            if !out.ends_with('#') {
                out.push('#');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_heading(t: &str) -> bool {
    t.starts_with('#') && t.trim_start_matches('#').starts_with(' ')
}

/// The page furniture of `source`.
pub(crate) fn furniture(source: &str) -> Furniture {
    let lines: Vec<&str> = source.lines().map(str::trim).collect();
    let blank = |i: Option<usize>| i.and_then(|i| lines.get(i)).map_or(true, |l| l.is_empty());
    let isolated = |i: usize| blank(i.checked_sub(1)) && blank(Some(i + 1));
    let mut shapes: HashMap<String, usize> = HashMap::new();
    // Printings of each line, and whether two of them are less than a page apart (a footer is
    // printed once per page, never in a run).
    let mut exact: HashMap<&str, (usize, usize, bool)> = HashMap::new();
    for (i, l) in lines.iter().enumerate() {
        if l.is_empty() || is_heading(l) {
            continue;
        }
        *shapes.entry(shape(l)).or_default() += 1;
        let e = exact.entry(l).or_insert((0, i, false));
        let close = e.0 > 0 && i < e.1 + MIN_PAGE_GAP;
        *e = (e.0 + 1, i, e.2 || close);
    }
    let repeated = |t: &str| shapes.get(&shape(t)).copied().unwrap_or(0) >= MIN_HEADER_REPEATS;
    let number =
        |i: usize, t: &str| marked_page_number(t) || (page_number(t) && isolated(i) && repeated(t));
    let header = |t: &str| repeated(t) && header_shape(t);
    let content = |t: &&str| !t.is_empty() && !is_heading(t);
    // Page marks: a page number or running header each.
    let marks: HashSet<usize> = (0..lines.len())
        .filter(|&i| content(&lines[i]) && (number(i, lines[i]) || header(lines[i])))
        .collect();
    // A footer sentence stands alone between blank lines and page marks, on many pages, in a
    // source that marks its pages.
    let apart = |k: Option<usize>| k.map_or(true, |k| blank(Some(k)) || marks.contains(&k));
    let mut alone: HashMap<&str, usize> = HashMap::new();
    for i in (0..lines.len()).filter(|&i| content(&lines[i])) {
        if apart(i.checked_sub(1)) && apart(Some(i + 1)) {
            *alone.entry(lines[i]).or_default() += 1;
        }
    }
    let mut out = Furniture::default();
    for (i, t) in lines.iter().enumerate().filter(|(_, t)| content(t)) {
        let n = exact.get(t).filter(|e| !e.2).map_or(0, |e| e.0);
        let words = t.split_whitespace().count();
        let footer = !marks.is_empty()
            && n >= MIN_FOOTER_REPEATS
            && alone.get(t).copied().unwrap_or(0) >= MIN_FOOTER_REPEATS
            && words >= MIN_FOOTER_WORDS
            && !t.contains(" | ");
        let text = header(t) || footer || (n >= MIN_HEADER_REPEATS && web_line(t));
        if marks.contains(&i) || text || debris(t) {
            out.lines.insert(i);
        }
        // Only a text of several words is taken out of a written description.
        if text && words >= 2 && !out.texts.iter().any(|x| x == t) {
            out.texts.push((*t).to_string());
        }
    }
    out
}

/// `html` without the furniture `texts` (a paragraph or line of its own, or inside a sentence),
/// and without the paragraphs left empty. `None` when nothing changed.
pub(crate) fn strip_texts(html: &str, texts: &[String]) -> Option<String> {
    let mut out = html.to_string();
    for t in texts {
        let escaped = t
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        for v in [t.as_str(), escaped.as_str()] {
            if v.trim().is_empty() || !out.contains(v) {
                continue;
            }
            out = out.replace(v, "");
        }
    }
    if out == html {
        return None;
    }
    let mut tidy = out.replace("<br><br>", "<br>").replace("<p><br>", "<p>");
    tidy = tidy.replace("<br></p>", "</p>");
    loop {
        let next = tidy
            .replace("  ", " ")
            .replace("<p> ", "<p>")
            .replace(" </p>", "</p>");
        let next = next.replace("<p></p>", "");
        if next == tidy {
            break;
        }
        tidy = next;
    }
    Some(tidy)
}

/// Most non-blank lines of a page break inside a table (a footnote, the running header or
/// footer, a page number).
const MAX_BREAK_PRINTED: usize = 4;
/// Most lines of such a page break, blank lines included.
const MAX_BREAK_LINES: usize = 12;

/// The number of cells of a table row, `None` for any other line.
fn row_width(line: &str) -> Option<usize> {
    let t = line.trim();
    t.contains(" | ")
        .then(|| t.trim_matches('|').split('|').count())
}

/// The first row (0-based) of the run of rows `row` is in: rows with single blank lines
/// between them, as an extraction prints them.
fn run_top(lines: &[&str], row: usize) -> usize {
    let is_row = |k: usize| lines.get(k).is_some_and(|l| row_width(l).is_some());
    let mut top = row;
    loop {
        if top >= 1 && is_row(top - 1) {
            top -= 1;
        } else if top >= 2 && lines[top - 1].trim().is_empty() && is_row(top - 2) {
            top -= 2;
        } else {
            return top;
        }
    }
}

/// The last row of the same table printed before a page break, for the row at `row` (0-based)
/// on the page after it: the run of rows `row` is in opens with a data row (it prints a digit;
/// a fresh table's header prints none), and above it, past a page break of no heading and no
/// row (a footnote, the running header, a page number: at most [`MAX_BREAK_PRINTED`] printed
/// lines), a row with the same number of cells ends the table on the page before. The table's
/// header is above that row. `None` otherwise.
pub(crate) fn row_before_break(lines: &[&str], row: usize) -> Option<usize> {
    let first = run_top(lines, row);
    let width = row_width(lines.get(first)?)?;
    if !lines[first].chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut printed = 0;
    for k in (first.saturating_sub(MAX_BREAK_LINES + 1)..first).rev() {
        let t = lines[k].trim();
        if let Some(w) = row_width(t) {
            return (w == width && printed > 0).then_some(k);
        }
        if t.starts_with('#') {
            return None;
        }
        if !t.is_empty() {
            printed += 1;
            if printed > MAX_BREAK_PRINTED {
                return None;
            }
        }
    }
    None
}

/// Words of a transcription marker ("(Summary)", "[Paraphrased]", "[Summary: ...]") that a
/// page transcription puts on text it did not copy word for word.
const MARKER_WORDS: [&str; 5] = [
    "summary",
    "summarised",
    "summarized",
    "paraphrased",
    "paraphrase",
];

/// The marker word a bracketed text is ("Summary", "paraphrased."), with what follows a
/// colon inside it ("Summary: text" -> "text").
fn marker(inside: &str) -> Option<&str> {
    let (word, rest) = match inside.split_once(':') {
        Some((w, r)) => (w, Some(r)),
        None => (inside, None),
    };
    let w = word.trim().trim_end_matches('.').to_lowercase();
    MARKER_WORDS
        .contains(&w.as_str())
        .then(|| rest.map_or("", str::trim))
}

/// Emphasis a transcription wraps a marker label in ("*Summary:*", "<em>Summary</em>:").
const LABEL_WRAPS: [(&str, &str); 6] = [
    ("**", "**"),
    ("__", "__"),
    ("*", "*"),
    ("_", "_"),
    ("<em>", "</em>"),
    ("<i>", "</i>"),
];

/// Every emphasised marker label ("*summary:*", "*summary*:", lower case), longest first.
fn label_forms() -> Vec<String> {
    let mut out: Vec<String> = MARKER_WORDS
        .iter()
        .flat_map(|w| {
            LABEL_WRAPS
                .iter()
                .flat_map(move |(o, c)| [format!("{o}{w}:{c}"), format!("{o}{w}{c}:")])
        })
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.len()));
    out
}

/// `html` without emphasised marker labels a transcription opens a text with ("*Summary:* The
/// races ..." -> "The races ..."): a label at the start, after a tag or after a space. `None`
/// when there is none.
fn strip_labels(html: &str) -> Option<String> {
    let forms = label_forms();
    let mut out = html.to_string();
    let mut changed = false;
    loop {
        let low = out.to_ascii_lowercase();
        let found = forms.iter().find_map(|f| {
            low.match_indices(f.as_str())
                .map(|(k, _)| k)
                .find(|&k| k == 0 || low[..k].ends_with(['>', ' ']))
                .map(|k| (k, f.len()))
        });
        let Some((k, len)) = found else { break };
        let mut end = k + len;
        if out[end..].starts_with(' ') {
            end += 1;
        }
        out.replace_range(k..end, "");
        changed = true;
    }
    changed.then_some(out)
}

/// `html` without transcription markers: a bare marker is left out, a marker around a text
/// ("[Summary: text]") keeps the text, an emphasised label ("*Summary:*") is left out. `None`
/// when there is none.
pub(crate) fn strip_markers(html: &str) -> Option<String> {
    let labelled = strip_labels(html);
    let html = labelled.as_deref().unwrap_or(html);
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut changed = labelled.is_some();
    while let Some(k) = rest.find(['(', '[']) {
        let close = if rest[k..].starts_with('(') { ')' } else { ']' };
        let found = rest[k + 1..]
            .find(close)
            .map(|e| k + 1 + e)
            .filter(|e| !rest[k + 1..*e].contains(['<', '(', '[']));
        let Some(text) = found.and_then(|e| marker(&rest[k + 1..e]).map(|t| (e, t))) else {
            out.push_str(&rest[..=k]);
            rest = &rest[k + 1..];
            continue;
        };
        let (e, text) = text;
        changed = true;
        out.push_str(&rest[..k]);
        out.push_str(text);
        rest = &rest[e + 1..];
        if text.is_empty() && (out.is_empty() || out.ends_with([' ', '>'])) {
            rest = rest.strip_prefix(' ').unwrap_or(rest);
        }
    }
    out.push_str(rest);
    if !changed {
        return None;
    }
    let tidy = out.replace(" </p>", "</p>").replace(" .", ".");
    Some(tidy.replace("<p></p>", ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eight pages of a lamp book: a footer, a running header with its page number and a web
    /// line on each page, a repeated rules sentence, debris and a price table.
    fn src() -> String {
        let mut s = String::from("## Lamps\n\nA lamp burns oil.\n\n");
        for page in 2..10 {
            if page % 2 == 0 {
                s.push_str("Once per day you can light it.\n\n");
            }
            s.push_str("Oil is thick.\nIt burns.\n\n");
            s.push_str("You gain a bonus to attack rolls made with\nthis lamp.\n\n");
            s.push_str("Read more at lampbook.example\n4 | Smoke oil\n\n");
            s.push_str("Not to be sold. Printed for your own table only.\n\n");
            s.push_str(&format!("LAMP BOOK | {page}!\n\n' ' '\n\n"));
        }
        s.push_str("Item | Cost\nLamp | 2\nOil | 2\nWick | 2\n");
        s
    }

    #[test]
    fn page_numbers_running_headers_footers_and_debris_are_furniture() {
        let src = src();
        let f = furniture(&src);
        let lines: Vec<&str> = src.lines().collect();
        let marked: Vec<&str> = (0..lines.len())
            .filter(|i| f.has(*i))
            .map(|i| lines[i])
            .collect();
        assert!(marked.contains(&"LAMP BOOK | 3!"), "{marked:?}");
        assert!(marked.contains(&"Read more at lampbook.example"));
        assert!(marked.contains(&"' ' '"));
        assert!(marked.contains(&"Not to be sold. Printed for your own table only."));
        // A repeated rules sentence (too few times) and a price table are content.
        assert!(!marked.iter().any(|l| l.starts_with("Once per day")));
        assert!(
            !marked.iter().any(|l| l.starts_with("You gain")),
            "a wrapped rules sentence on every page is no footer"
        );
        assert!(!marked
            .iter()
            .any(|l| l.contains("| 2") && !l.contains("BOOK")));
        assert!(!marked.contains(&"Oil is thick."));
    }

    #[test]
    fn furniture_is_stripped_from_a_description() {
        let f = furniture(&src());
        let html =
            "<p>A lamp burns oil.</p><p>Not to be sold. Printed for your own table only.</p>\
<p>Read more at lampbook.example 4 | Smoke oil</p><p>LAMP BOOK | 2!</p>";
        assert_eq!(
            strip_texts(html, f.texts()).as_deref(),
            Some("<p>A lamp burns oil.</p><p>4 | Smoke oil</p>")
        );
        assert_eq!(strip_texts("<p>A lamp burns oil.</p>", f.texts()), None);
    }

    #[test]
    fn a_table_split_by_a_page_break_continues_under_its_header() {
        let lines = [
            "Tool | Cost | DC",
            "Awl | 1gp | 10",
            "",
            "Saw | 2gp | 15",
            "",
            "^ A saw dulls fast.",
            "Tool Stats",
            "",
            "Adze | 3gp | 20",
            "",
            "Plane | 4gp | 25",
        ];
        assert_eq!(row_before_break(&lines, 10), Some(3));
        assert_eq!(row_before_break(&lines, 8), Some(3));
        // On the first page there is no break; a fresh header prints no digit.
        assert_eq!(row_before_break(&lines, 3), None);
        let fresh = ["a | b", "1 | 2", "", "Note.", "", "Name | Kind", "x | 3"];
        assert_eq!(row_before_break(&fresh, 6), None);
        // A heading or a table of another width in between is another table.
        let headed = ["a | b", "1 | 2", "## Next", "3 | 4"];
        assert_eq!(row_before_break(&headed, 3), None);
        let wider = ["a | b | c", "1 | 2 | 3", "Note.", "4 | 5"];
        assert_eq!(row_before_break(&wider, 3), None);
    }

    #[test]
    fn transcription_markers_are_left_out_of_a_description() {
        assert_eq!(
            strip_markers(
                "<p>(Summary) Owls hoot.</p><p><strong>Age.</strong> (Paraphrased.) Old.</p>"
            )
            .as_deref(),
            Some("<p>Owls hoot.</p><p><strong>Age.</strong> Old.</p>")
        );
        assert_eq!(
            strip_markers("<p>[Summary: Owls hoot at night.] They roost.</p>").as_deref(),
            Some("<p>Owls hoot at night. They roost.</p>")
        );
        assert_eq!(strip_markers("<p>Owls (see summary) hoot [1].</p>"), None);
    }

    #[test]
    fn emphasised_marker_labels_are_left_out_but_a_plain_word_stays() {
        assert_eq!(
            strip_markers("<p><strong>Owls</strong></p><p>*Summary:* Owls hoot.</p>").as_deref(),
            Some("<p><strong>Owls</strong></p><p>Owls hoot.</p>")
        );
        assert_eq!(
            strip_markers("<p>**Paraphrased:** Owls hoot. <em>Summary</em>: They roost.</p>")
                .as_deref(),
            Some("<p>Owls hoot. They roost.</p>")
        );
        assert_eq!(
            strip_markers("<p>*Summary:*</p><p>Owls.</p>").as_deref(),
            Some("<p>Owls.</p>")
        );
        assert_eq!(
            strip_markers("<p>Summary: owls hoot. A summary*: no.</p>"),
            None
        );
    }
}
