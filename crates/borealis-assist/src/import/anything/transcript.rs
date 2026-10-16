//! What a visual transcription says about itself. A model copying a page may paraphrase a
//! passage and mark it ("[paraphrased]"), disclaim the whole copy, mark print it cannot read
//! ("[illegible]") or add a note of its own ("[Note: page 1 is the cover ...]"). The markers and
//! notes never reach the source text as printed words; the passages they mark are recorded, page
//! by page, so the review can flag every element whose text holds one ([`not_verbatim_spans`]).

use crate::import::chunk::norm_words;

/// Prefix of a detection note recording one passage a transcription did not copy word for word
/// (reworded, or with words it could not read): the reading note prefix, then
/// `paraphrased passage: "<passage>" (<pages>)`, the form the review's paraphrase check reads.
/// Read it back with [`not_verbatim_spans`].
pub(crate) const NOT_VERBATIM_NOTE: &str = "Reading the source: paraphrased passage: ";
/// What a transcription writes where the print cannot be read.
pub(crate) const ILLEGIBLE: &str = "[illegible]";

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
/// A marked line with fewer words than this (a marker on a line of its own) stands for its
/// paragraph.
const MIN_SPAN_WORDS: usize = 3;
/// Words (in order) a text must share with a recorded passage to hold it: a sentence's worth.
const MATCH_WORDS: usize = 8;
/// Openings of a transcriber's note in parentheses (square-bracketed notes need none).
const NOTE_OPENINGS: [&str; 4] = ["note", "transcriber", "image", "illustration"];
/// Fewest words of a bracketed paragraph that is a note (a lone "[illegible]" is not one).
const MIN_NOTE_WORDS: usize = 3;

/// A transcription read for what it says about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Transcript {
    /// The text without disclaimer, paraphrase markers and transcriber notes.
    pub text: String,
    /// Passages not copied word for word (reworded, or holding [`ILLEGIBLE`]), as kept in
    /// `text`.
    pub spans: Vec<String>,
    /// The transcription disclaimed being a word-for-word copy, or marked a passage reworded.
    pub paraphrased: bool,
}

fn words(t: &str) -> usize {
    t.split_whitespace().count()
}

/// `t` without the emphasis or quote marks around it.
fn bare(t: &str) -> &str {
    t.trim()
        .trim_start_matches('>')
        .trim()
        .trim_matches(|c| c == '*' || c == '_')
        .trim()
}

/// A paragraph that is the transcriber's own note, not printed text: wholly in square brackets
/// (`[Note: ...]`, `[The page shows ...]`), or in parentheses opening with "Note", "Image" and
/// the like. A lone `[illegible]` is a marker, kept.
pub(crate) fn is_transcriber_note(paragraph: &str) -> bool {
    let t = bare(paragraph);
    if words(t) < MIN_NOTE_WORDS {
        return false;
    }
    if t.starts_with('[') && t.ends_with(']') {
        return !t[1..t.len() - 1].contains(['[', ']']);
    }
    let lower = t.to_lowercase();
    lower.starts_with('(')
        && lower.ends_with(')')
        && NOTE_OPENINGS
            .iter()
            .any(|o| lower[1..].trim_start().starts_with(o))
}

/// The passage a marked line stands for: the line, or its paragraph when the line is short (a
/// marker on a line of its own).
fn span_of(line: &str, paragraph: &str) -> String {
    let line = line.trim();
    if words(line) >= MIN_SPAN_WORDS {
        line.to_string()
    } else {
        paragraph.trim().to_string()
    }
}

/// Read `text` (one transcription): drop its disclaimer paragraph, its transcriber notes and
/// its paraphrase markers, and record the passages it did not copy word for word.
pub(crate) fn read(text: &str) -> Transcript {
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    // A disclaimer speaks about the copy; a passage merely marked as reworded is printed text.
    let disclaimer = paragraphs.iter().take(DISCLAIMER_PARAGRAPHS).position(|p| {
        let lower = unmark(p).0.to_lowercase();
        !p.trim_start().starts_with('#') && DISCLAIMER_MARKS.iter().any(|m| lower.contains(m))
    });
    let mut out = Transcript {
        paraphrased: disclaimer.is_some(),
        ..Transcript::default()
    };
    let mut kept: Vec<String> = Vec::new();
    for (i, p) in paragraphs.iter().enumerate() {
        if Some(i) == disclaimer {
            continue;
        }
        // A transcriber's note (a paragraph, or a line inside one) is no printed text.
        if is_transcriber_note(p) {
            continue;
        }
        let lines: Vec<&str> = p.lines().collect();
        let own: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|l| !is_transcriber_note(l))
            .collect();
        if own.is_empty() && !lines.is_empty() {
            continue;
        }
        let paragraph = own.join("\n");
        let mut cleaned: Vec<String> = Vec::new();
        let mut marked_lines: Vec<String> = Vec::new();
        for line in &own {
            let (clean, marked) = unmark(line);
            if marked || clean.contains(ILLEGIBLE) {
                marked_lines.push(clean.clone());
            }
            out.paraphrased |= marked;
            cleaned.push(clean);
        }
        let clean_paragraph = cleaned.join("\n");
        for l in marked_lines {
            let span = span_of(&l, &unmark(&paragraph).0);
            if !span.is_empty() && !out.spans.contains(&span) {
                out.spans.push(span);
            }
        }
        kept.push(clean_paragraph);
    }
    out.text = kept.join("\n\n");
    if disclaimer.is_some() {
        // Nothing of a disclaimed copy is known to be printed word for word.
        for p in out.text.split("\n\n") {
            let p = p.trim();
            if words(p) >= MIN_SPAN_WORDS
                && !p.starts_with('#')
                && !out.spans.iter().any(|s| s == p)
            {
                out.spans.push(p.to_string());
            }
        }
    }
    out
}

/// `line` without its paraphrase markers, and whether it had one.
fn unmark(line: &str) -> (String, bool) {
    let mut out = line.to_string();
    let mut marked = false;
    for mark in PARAPHRASE_MARKS {
        while let Some(at) = out.to_lowercase().find(mark) {
            let end = at + mark.len();
            let skip = out[end..].len() - out[end..].trim_start_matches([' ', '\t']).len();
            out.replace_range(at..end + skip, "");
            marked = true;
        }
    }
    (out, marked)
}

/// The draft notes recording each passage of `spans`, read from `label` (a page or a part).
pub(crate) fn span_notes(label: &str, spans: &[String]) -> Vec<String> {
    spans
        .iter()
        .map(|s| format!("{NOT_VERBATIM_NOTE}\"{s}\" ({label})"))
        .collect()
}

/// The passages (where, passage) recorded in `notes` (the detection's notes) as not copied word
/// for word.
pub(crate) fn not_verbatim_spans(notes: &[String]) -> Vec<(String, String)> {
    notes
        .iter()
        .filter_map(|n| n.strip_prefix(NOT_VERBATIM_NOTE))
        .filter_map(|n| {
            let (quoted, place) = n.rsplit_once("\" (")?;
            let passage = quoted.strip_prefix('"')?;
            Some((place.strip_suffix(')')?.to_string(), passage.to_string()))
        })
        .collect()
}

/// The first recorded passage `text` (an element's description, HTML or plain) holds: it shares
/// [`MATCH_WORDS`] words in a row with it (all of a shorter passage's words).
pub(crate) fn holds_not_verbatim<'a>(text: &str, spans: &'a [(String, String)]) -> Option<&'a str> {
    let hay = format!(" {} ", norm_words(&strip_tags(text)));
    spans.iter().map(|(_, s)| s.as_str()).find(|s| {
        let w: Vec<String> = norm_words(s)
            .split(' ')
            .filter(|w| !w.is_empty() && *w != "illegible")
            .map(str::to_string)
            .collect();
        if w.is_empty() {
            return false;
        }
        let n = MATCH_WORDS.min(w.len());
        w.windows(n)
            .any(|win| hay.contains(&format!(" {} ", win.join(" "))))
    })
}

/// `html` with its tags replaced by spaces.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
