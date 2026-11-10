//! The escape lint (spec 7.3): text that looks like an escape artefact from pasting, and its one-tap fix.
//!
//! Looked for in the characters the user sees (the tree's text, after the parser decoded the HTML once):
//! * backslash escapes: `\n`, `\t`, `\r`, `\"`, `\'`, `\/`, `\uXXXX` (any number of backslashes before them);
//! * HTML character references shown as text: `&amp;` (from `&amp;amp;`), `&quot;`, `&#8217;`, `&nbsp;`...;
//! * mojibake: UTF-8 read as Windows-1252, such as `â€™` for `’`;
//! * any other (stray) backslash.
//!
//! The fix replaces each with the real character (`\n` becomes a line break, `&nbsp;` and `\t` a space, a stray
//! backslash nothing) and repeats until nothing is found, so `&amp;amp;amp;` ends as `&`. Kept blocks are not
//! looked at: they are the original bytes by definition.

use serde::{Deserialize, Serialize};

use super::entities;
use super::{Block, Cell, Doc, Inline};

/// Fix passes before giving up (each pass removes one layer of escaping).
const MAX_FIX_PASSES: usize = 8;

/// Windows-1252 bytes 0x80..=0x9F; the five undefined bytes map to the C1 control of the same number.
const CP1252_HIGH: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
];

/// What kind of artefact was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscapeKind {
    /// `\n`, `\"`, `\u2019`...
    BackslashEscape,
    /// `&amp;`, `&#8217;`, `&quot;`... shown as text.
    HtmlEntity,
    /// UTF-8 read as Windows-1252 (`â€™`).
    Mojibake,
    /// A backslash that escapes nothing.
    StrayBackslash,
}

/// One artefact in a string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFinding {
    /// Kind.
    pub kind: EscapeKind,
    /// Byte offset in the text.
    pub start: usize,
    /// The artefact as it appears.
    pub found: String,
    /// What the fix writes instead.
    pub replacement: String,
}

/// One artefact in a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeFinding {
    /// Indices from the document down to the text node: block, then list item / row and cell / child block, then
    /// the inline node.
    pub path: Vec<usize>,
    /// Kind.
    pub kind: EscapeKind,
    /// The artefact as it appears.
    pub found: String,
    /// What the fix writes instead.
    pub replacement: String,
}

/// Find escape artefacts in `text`.
#[must_use]
pub fn lint_text(text: &str) -> Vec<TextFinding> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        let rest = &text[i..];
        let hit = match c {
            '\\' => Some(backslash_at(rest)),
            '&' => entities::decode_at(rest).map(|(ch, n)| {
                let replacement = if ch == '\u{a0}' {
                    " ".to_owned()
                } else {
                    ch.to_string()
                };
                (EscapeKind::HtmlEntity, n, replacement)
            }),
            _ => mojibake_at(rest).map(|(n, ch)| (EscapeKind::Mojibake, n, ch.to_string())),
        };
        match hit {
            Some((kind, n, replacement)) => {
                out.push(TextFinding {
                    kind,
                    start: i,
                    found: rest[..n].to_owned(),
                    replacement,
                });
                i += n;
            }
            None => i += c.len_utf8(),
        }
    }
    out
}

/// Replace every artefact in `text`, repeating until none is left.
#[must_use]
pub fn fix_text(text: &str) -> String {
    let mut current = text.to_owned();
    for _ in 0..MAX_FIX_PASSES {
        let findings = lint_text(&current);
        if findings.is_empty() {
            break;
        }
        let mut next = String::with_capacity(current.len());
        let mut at = 0;
        for f in &findings {
            next.push_str(&current[at..f.start]);
            next.push_str(&f.replacement);
            at = f.start + f.found.len();
        }
        next.push_str(&current[at..]);
        current = next;
    }
    current
}

/// Find escape artefacts in the editable text of `doc`.
#[must_use]
pub fn lint(doc: &Doc) -> Vec<EscapeFinding> {
    let mut out = Vec::new();
    for (i, block) in doc.content.iter().enumerate() {
        lint_block(block, &mut vec![i], &mut out);
    }
    out
}

/// `doc` with every artefact fixed ([`fix_text`]; a fixed line break becomes a hard break), normalized.
#[must_use]
pub fn fix(doc: &Doc) -> Doc {
    Doc::new(doc.content.iter().map(fix_block).collect()).normalized()
}

fn lint_block(block: &Block, path: &mut Vec<usize>, out: &mut Vec<EscapeFinding>) {
    match block {
        Block::Paragraph { content, .. } | Block::Heading { content, .. } => {
            lint_inline(content, path, out)
        }
        Block::BulletList { content, .. } | Block::OrderedList { content, .. } => {
            for (i, item) in content.iter().enumerate() {
                path.push(i);
                lint_inline(&item.content, path, out);
                path.pop();
            }
        }
        Block::Table { content, .. } => {
            for (r, row) in content.iter().enumerate() {
                for (c, cell) in row.content.iter().enumerate() {
                    path.extend([r, c]);
                    let (Cell::Data { content } | Cell::Header { content }) = cell;
                    lint_inline(content, path, out);
                    path.truncate(path.len() - 2);
                }
            }
        }
        Block::Blockquote { content, .. } => {
            for (i, child) in content.iter().enumerate() {
                path.push(i);
                lint_block(child, path, out);
                path.pop();
            }
        }
        Block::Kept { .. } => {}
    }
}

fn lint_inline(nodes: &[Inline], path: &[usize], out: &mut Vec<EscapeFinding>) {
    for (i, node) in nodes.iter().enumerate() {
        if let Inline::Text { text, .. } = node {
            for f in lint_text(text) {
                let mut at = path.to_vec();
                at.push(i);
                out.push(EscapeFinding {
                    path: at,
                    kind: f.kind,
                    found: f.found,
                    replacement: f.replacement,
                });
            }
        }
    }
}

fn fix_block(block: &Block) -> Block {
    match block {
        Block::Paragraph { attrs, content } => Block::Paragraph {
            attrs: attrs.clone(),
            content: fix_inline(content),
        },
        Block::Heading { attrs, content } => Block::Heading {
            attrs: attrs.clone(),
            content: fix_inline(content),
        },
        Block::BulletList { attrs, content } => Block::BulletList {
            attrs: attrs.clone(),
            content: fix_items(content),
        },
        Block::OrderedList { attrs, content } => Block::OrderedList {
            attrs: attrs.clone(),
            content: fix_items(content),
        },
        Block::Table { attrs, content } => {
            let rows = content
                .iter()
                .map(|row| {
                    let mut row = row.clone();
                    for cell in &mut row.content {
                        let (Cell::Data { content } | Cell::Header { content }) = cell;
                        *content = fix_inline(content);
                    }
                    row
                })
                .collect();
            Block::Table {
                attrs: attrs.clone(),
                content: rows,
            }
        }
        Block::Blockquote { attrs, content } => Block::Blockquote {
            attrs: attrs.clone(),
            content: content.iter().map(fix_block).collect(),
        },
        Block::Kept { .. } => block.clone(),
    }
}

fn fix_items(items: &[super::ListItem]) -> Vec<super::ListItem> {
    items
        .iter()
        .map(|item| super::ListItem {
            content: fix_inline(&item.content),
        })
        .collect()
}

fn fix_inline(nodes: &[Inline]) -> Vec<Inline> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Inline::Text { text, marks } = node else {
            out.push(node.clone());
            continue;
        };
        let fixed = fix_text(text);
        for (i, line) in fixed.split('\n').enumerate() {
            if i > 0 {
                out.push(Inline::HardBreak {
                    marks: marks.clone(),
                });
            }
            out.push(Inline::Text {
                text: line.to_owned(),
                marks: marks.clone(),
            });
        }
    }
    out
}

/// The backslash escape at the start of `rest` (which starts with `\`): kind, length, replacement.
fn backslash_at(rest: &str) -> (EscapeKind, usize, String) {
    let run = rest.bytes().take_while(|b| *b == b'\\').count();
    let after = &rest[run..];
    let simple = match after.chars().next() {
        Some('n') => Some("\n"),
        Some('r') => Some(""),
        Some('t') => Some(" "),
        Some('"') => Some("\""),
        Some('\'') => Some("'"),
        Some('/') => Some("/"),
        _ => None,
    };
    if let Some(replacement) = simple {
        return (EscapeKind::BackslashEscape, run + 1, replacement.to_owned());
    }
    if let Some((n, replacement)) = unicode_escape(after) {
        return (EscapeKind::BackslashEscape, run + n, replacement);
    }
    (EscapeKind::StrayBackslash, run, String::new())
}

/// `uXXXX` (optionally a surrogate pair `uD83D\uDE00`) at the start of `s`: length and the character.
fn unicode_escape(s: &str) -> Option<(usize, String)> {
    let hex4 = |t: &str| -> Option<u32> {
        let digits = t.strip_prefix('u')?.get(..4)?;
        if digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            u32::from_str_radix(digits, 16).ok()
        } else {
            None
        }
    };
    let first = hex4(s)?;
    if (0xD800..0xDC00).contains(&first) {
        if let Some(low) = s.get(5..).and_then(|t| t.strip_prefix('\\')).and_then(hex4) {
            if (0xDC00..0xE000).contains(&low) {
                let code = 0x10000 + ((first - 0xD800) << 10) + (low - 0xDC00);
                return Some((
                    11,
                    char::from_u32(code).map(String::from).unwrap_or_default(),
                ));
            }
        }
    }
    Some((
        5,
        char::from_u32(first)
            .filter(|c| *c != '\0')
            .map(String::from)
            .unwrap_or_default(),
    ))
}

fn cp1252_byte(c: char) -> Option<u8> {
    let code = u32::from(c);
    if (0x80..=0xFF).contains(&code) {
        return u8::try_from(code).ok();
    }
    let i = CP1252_HIGH.iter().position(|x| *x == c)?;
    u8::try_from(0x80 + i).ok()
}

/// UTF-8 bytes shown as Windows-1252 characters at the start of `s`: length in `s` and the real character.
fn mojibake_at(s: &str) -> Option<(usize, char)> {
    let mut chars = s.char_indices();
    let lead = cp1252_byte(chars.next()?.1)?;
    let continuation = match lead {
        0xC2..=0xDF => 1,
        0xE0..=0xEF => 2,
        0xF0..=0xF4 => 3,
        _ => return None,
    };
    let mut bytes = vec![lead];
    for _ in 0..continuation {
        let b = cp1252_byte(chars.next()?.1)?;
        if !(0x80..=0xBF).contains(&b) {
            return None;
        }
        bytes.push(b);
    }
    let decoded = std::str::from_utf8(&bytes).ok()?;
    let ch = decoded.chars().next()?;
    let len = chars.next().map_or(s.len(), |(i, _)| i);
    Some((len, ch))
}

#[cfg(test)]
mod tests {
    use super::super::{parse, render, BoldTag, Mark};
    use super::*;

    fn kinds(text: &str) -> Vec<(EscapeKind, String, String)> {
        lint_text(text)
            .into_iter()
            .map(|f| (f.kind, f.found, f.replacement))
            .collect()
    }

    #[test]
    fn finds_backslash_escapes() {
        use EscapeKind::{BackslashEscape as B, StrayBackslash as S};
        assert_eq!(
            kinds(r#"a\nb \"q\" it\'s \\n \t\u2019 \x \"#),
            vec![
                (B, r"\n".into(), "\n".into()),
                (B, r#"\""#.into(), "\"".into()),
                (B, r#"\""#.into(), "\"".into()),
                (B, r"\'".into(), "'".into()),
                (B, r"\\n".into(), "\n".into()),
                (B, r"\t".into(), " ".into()),
                (B, r"\u2019".into(), "\u{2019}".into()),
                (S, r"\".into(), String::new()),
                (S, r"\".into(), String::new()),
            ]
        );
        assert_eq!(
            kinds(r"\uD83D\uDE00"),
            vec![(B, r"\uD83D\uDE00".into(), "\u{1f600}".into())]
        );
    }

    #[test]
    fn finds_entities_and_mojibake() {
        use EscapeKind::{HtmlEntity as E, Mojibake as M};
        assert_eq!(
            kinds(
                "Tom &amp; Jerry&#8217;s &quot;x&quot;&nbsp;\u{e2}\u{20ac}\u{2122} caf\u{c3}\u{a9}"
            ),
            vec![
                (E, "&amp;".into(), "&".into()),
                (E, "&#8217;".into(), "\u{2019}".into()),
                (E, "&quot;".into(), "\"".into()),
                (E, "&quot;".into(), "\"".into()),
                (E, "&nbsp;".into(), " ".into()),
                (M, "\u{e2}\u{20ac}\u{2122}".into(), "\u{2019}".into()),
                (M, "\u{c3}\u{a9}".into(), "\u{e9}".into()),
            ]
        );
    }

    #[test]
    fn ordinary_text_is_clean() {
        for text in [
            "Fire & ice, \u{201c}quoted\u{201d}, na\u{ef}ve, S\u{e3}o Paulo, \u{c4}rger, 1/2, a;b, AT&T",
            "\u{2014} \u{2013} \u{2026} \u{d7} \u{bd} \u{c9}lan \u{fc}ber \u{c3}",
        ] {
            assert_eq!(lint_text(text), vec![], "{text}");
        }
    }

    #[test]
    fn fix_repeats_until_clean() {
        assert_eq!(fix_text("Arms &amp;amp;amp; Armor"), "Arms & Armor");
        assert_eq!(fix_text(r"line\\\nnext"), "line\nnext");
        assert_eq!(fix_text(r"a \\ b"), "a  b");
        assert_eq!(fix_text("it\u{e2}\u{20ac}\u{2122}s"), "it\u{2019}s");
    }

    #[test]
    fn doc_fix_turns_line_breaks_into_hard_breaks() {
        let doc = parse("<p><b>One\\nTwo</b> &amp;amp; three</p>\n<pre>\\n kept</pre>");
        let findings = lint(&doc);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].path, vec![0, 0]);
        assert_eq!(findings[1].path, vec![0, 1]);
        let fixed = fix(&doc);
        assert_eq!(
            render(&fixed),
            "<p><b>One<br />Two</b> &amp; three</p>\n<pre>\\n kept</pre>"
        );
        assert!(lint(&fixed).is_empty());
        let Block::Paragraph { content, .. } = &fixed.content[0] else {
            panic!("p")
        };
        assert_eq!(
            content[1],
            Inline::HardBreak {
                marks: vec![Mark::bold(BoldTag::B)]
            }
        );
    }

    #[test]
    fn doc_lint_paths_reach_lists_tables_and_quotes() {
        let doc = parse(
            "<ul><li>ok</li><li>a\\tb</li></ul>\n<table><tr><td>x</td><td>&amp;quot;</td></tr></table>\n\
             <blockquote><p>\\\"</p></blockquote>",
        );
        let paths: Vec<Vec<usize>> = lint(&doc).into_iter().map(|f| f.path).collect();
        assert_eq!(paths, vec![vec![0, 1, 0], vec![1, 0, 1, 0], vec![2, 0, 0]]);
    }
}
