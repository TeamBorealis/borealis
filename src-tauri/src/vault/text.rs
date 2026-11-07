//! Note bytes <-> text. Obsidian notes are UTF-8, with or without a byte order mark, with LF or CRLF line endings.
//! A note is read as it is; the editor works on LF text (a browser text box turns CRLF into LF), and a save puts the
//! note's own line ending and BOM back, so an edit changes only the lines the user changed. A file that is not UTF-8
//! opens read-only.

use serde::Serialize;
use sha2::{Digest, Sha256};

/// The UTF-8 byte order mark.
pub const BOM: &[u8] = b"\xEF\xBB\xBF";

/// The line ending a note is written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding {
    Lf,
    Crlf,
}

/// A note's bytes as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The text after the BOM, line endings as in the file (lossy when `utf8` is false).
    pub text: String,
    pub bom: bool,
    /// The line ending most lines use (LF when there is no line break).
    pub line_ending: LineEnding,
    /// The bytes are valid UTF-8.
    pub utf8: bool,
}

/// The line ending most lines of `text` end with; LF on a tie or without line breaks.
#[must_use]
pub fn line_ending(text: &str) -> LineEnding {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    if crlf > lf {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    }
}

/// Decode note bytes.
#[must_use]
pub fn decode(bytes: &[u8]) -> Decoded {
    let (bom, body) = match bytes.strip_prefix(BOM) {
        Some(rest) => (true, rest),
        None => (false, bytes),
    };
    let (text, utf8) = match std::str::from_utf8(body) {
        Ok(t) => (t.to_string(), true),
        Err(_) => (String::from_utf8_lossy(body).into_owned(), false),
    };
    Decoded {
        line_ending: line_ending(&text),
        text,
        bom,
        utf8,
    }
}

/// Encode edited text for a note: every line break becomes `line_ending`, and the BOM is put back when the note had one.
#[must_use]
pub fn encode(text: &str, bom: bool, line_ending: LineEnding) -> Vec<u8> {
    let lf = text.replace("\r\n", "\n");
    let body = match line_ending {
        LineEnding::Lf => lf,
        LineEnding::Crlf => lf.replace('\n', "\r\n"),
    };
    encode_raw(&body, bom)
}

/// A line without its line ending.
fn content(line: &str) -> &str {
    line.strip_suffix('\n')
        .map_or(line, |l| l.strip_suffix('\r').unwrap_or(l))
}

/// Encode an edit of `original` (the note as read, line endings as in the file). The lines before and after the
/// changed part keep exactly the line endings they had; the changed lines get the note's main ending. For a file with
/// one kind of line ending this is the same as [`encode`]; for a file with mixed endings, an edit leaves the rest alone.
#[must_use]
pub fn encode_keeping(text: &str, original: &Decoded) -> Vec<u8> {
    let edited = text.replace("\r\n", "\n");
    let old: Vec<&str> = original.text.split_inclusive('\n').collect();
    let new: Vec<&str> = edited.split_inclusive('\n').collect();
    let same =
        |a: &str, b: &str| content(a) == content(b) && a.ends_with('\n') == b.ends_with('\n');
    let head = old.iter().zip(&new).take_while(|(a, b)| same(a, b)).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| same(a, b))
        .count();
    let ending = match original.line_ending {
        LineEnding::Lf => "\n",
        LineEnding::Crlf => "\r\n",
    };
    let mut out: String = old[..head].concat();
    for line in &new[head..new.len() - tail] {
        out.push_str(content(line));
        if line.ends_with('\n') {
            out.push_str(ending);
        }
    }
    out.push_str(&old[old.len() - tail..].concat());
    encode_raw(&out, original.bom)
}

/// Encode text whose line breaks are already right (link updates edit the file's own text): only the BOM is added.
#[must_use]
pub fn encode_raw(text: &str, bom: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + BOM.len());
    if bom {
        out.extend_from_slice(BOM);
    }
    out.extend_from_slice(text.as_bytes());
    out
}

/// The content hash used to detect changes made by other apps (SHA-256, hex).
#[must_use]
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The 1-based line of a byte offset in `text`.
#[must_use]
pub fn line_of(text: &str, offset: usize) -> u32 {
    let upto = text.get(..offset).unwrap_or(text);
    u32::try_from(upto.matches('\n').count() + 1).unwrap_or(u32::MAX)
}

/// The line (without its line break) that holds byte `offset`.
#[must_use]
pub fn line_around(text: &str, offset: usize) -> &str {
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    text[start..end].trim_end_matches('\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_and_bom_come_back_byte_for_byte_after_the_editor_turns_them_into_lf() {
        let original = b"\xEF\xBB\xBF---\r\ntags: [a]\r\n---\r\n# Title\r\n\r\nLine  \r\nlast";
        let d = decode(original);
        assert!(d.bom && d.utf8);
        assert_eq!(d.line_ending, LineEnding::Crlf);
        let in_editor = d.text.replace("\r\n", "\n");
        assert_eq!(encode(&in_editor, d.bom, d.line_ending), original);
    }

    #[test]
    fn lf_files_without_a_final_line_break_stay_as_they_are() {
        let original = b"one\ntwo\t\n\n  three";
        let d = decode(original);
        assert!(!d.bom);
        assert_eq!(d.line_ending, LineEnding::Lf);
        assert_eq!(encode(&d.text, d.bom, d.line_ending), original);
    }

    #[test]
    fn mixed_line_endings_use_the_more_common_one_and_ties_are_lf() {
        assert_eq!(line_ending("a\r\nb\r\nc\n"), LineEnding::Crlf);
        assert_eq!(line_ending("a\r\nb\n"), LineEnding::Lf);
        assert_eq!(line_ending("no breaks"), LineEnding::Lf);
    }

    #[test]
    fn an_edit_of_a_file_with_mixed_line_endings_leaves_the_other_lines_alone() {
        let original = b"\xEF\xBB\xBFone\r\ntwo\r\nthree\nfour\r\nfive\n";
        let d = decode(original);
        let in_editor = d.text.replace("\r\n", "\n");
        assert_eq!(encode_keeping(&in_editor, &d), original, "unchanged");
        let edited = in_editor.replace("three", "THREE");
        assert_eq!(
            encode_keeping(&edited, &d),
            b"\xEF\xBB\xBFone\r\ntwo\r\nTHREE\r\nfour\r\nfive\n",
            "only the edited line takes the main ending"
        );
        let appended = format!("{in_editor}six");
        assert_eq!(
            encode_keeping(&appended, &d),
            b"\xEF\xBB\xBFone\r\ntwo\r\nthree\nfour\r\nfive\nsix"
        );
        let removed = in_editor.replace("two\n", "");
        assert_eq!(
            encode_keeping(&removed, &d),
            b"\xEF\xBB\xBFone\r\nthree\nfour\r\nfive\n"
        );
    }

    #[test]
    fn keeping_endings_matches_plain_encoding_for_consistent_files() {
        for original in [&b"a\r\nb\r\nc"[..], b"a\nb\n", b"", b"x"] {
            let d = decode(original);
            let edited = format!("{}\nadded", d.text.replace("\r\n", "\n"));
            assert_eq!(
                encode_keeping(&edited, &d),
                encode(&edited, d.bom, d.line_ending)
            );
        }
    }

    #[test]
    fn text_that_is_not_utf8_is_flagged() {
        let d = decode(b"caf\xE9");
        assert!(!d.utf8);
        assert_eq!(d.text, "caf\u{FFFD}");
    }

    #[test]
    fn raw_encoding_keeps_line_breaks_and_adds_only_the_bom() {
        assert_eq!(encode_raw("a\r\nb\n", true), b"\xEF\xBB\xBFa\r\nb\n");
    }

    #[test]
    fn lines_are_found_by_offset() {
        let t = "one\r\ntwo [[x]]\nthree";
        let at = t.find("[[").unwrap();
        assert_eq!(line_of(t, at), 2);
        assert_eq!(line_around(t, at), "two [[x]]");
        assert_eq!(line_around(t, 0), "one");
    }

    #[test]
    fn the_hash_is_sha256_hex() {
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
