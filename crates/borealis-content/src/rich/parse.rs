//! HTML fragment to [`Doc`], with kept blocks for everything the editor does not own.
//!
//! The fragment is cut into top-level segments. A segment that is a palette block **and** that [`render`] writes
//! back to exactly the same bytes becomes an editable block; any other segment becomes a [`Block::Kept`] holding
//! its bytes. Separators between blocks are recorded (`joined`), so `render(&parse(html)) == html` for every input.

use super::entities;
use super::render::{render, write_block};
use super::{
    Block, BlockAttrs, Cell, Doc, HeadingAttrs, Inline, KeptReason, ListItem, Mark, ParagraphAttrs,
    RowAttrs, TableAttrs, TableRow,
};
use super::{BoldTag, ItalicTag};

/// Tags the editor's palette holds (intersection of the backend sanitizer and the UI's `SafeHtml`).
const PALETTE: [&str; 18] = [
    "p",
    "b",
    "strong",
    "i",
    "em",
    "br",
    "ul",
    "ol",
    "li",
    "h3",
    "h4",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "blockquote",
];

/// Tags that sit inside a line of text; every other tag starts a block.
const INLINE_TAGS: [&str; 26] = [
    "b", "strong", "i", "em", "br", "span", "a", "u", "s", "sup", "sub", "small", "code", "font",
    "mark", "abbr", "cite", "q", "del", "ins", "kbd", "var", "img", "wbr", "big", "tt",
];

const VOID_TAGS: [&str; 6] = ["br", "hr", "img", "wbr", "input", "meta"];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Start {
        name: String,
        attrs: bool,
        self_closing: bool,
    },
    End {
        name: String,
    },
    Text,
    /// Comments, doctypes, processing instructions.
    Other,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    start: usize,
    end: usize,
}

/// Parse an HTML fragment. Never fails: what the editor does not own is kept.
#[must_use]
pub fn parse(html: &str) -> Doc {
    let toks = tokenize(html);
    if toks.is_empty() {
        return Doc::default();
    }
    if !toks.iter().any(|t| starts_block(&t.tok)) {
        return Doc::new(vec![inline_only(html, &toks)]);
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut loose_from: Option<usize> = None;
    let mut i = 0;
    while i < toks.len() {
        if !starts_block(&toks[i].tok) {
            loose_from.get_or_insert(i);
            i += 1;
            continue;
        }
        let joined_next = match loose_from.take() {
            Some(from) => {
                let text = &html[toks[from].start..toks[i].start];
                let gap = text == "\n" && !blocks.is_empty();
                if !gap {
                    push(&mut blocks, Block::kept(text, reason(&toks[from..i])), true);
                }
                !gap
            }
            None => true,
        };
        let Some(j) = matching_end(&toks, i) else {
            let rest = &html[toks[i].start..];
            push(
                &mut blocks,
                Block::kept(rest, reason(&toks[i..])),
                joined_next,
            );
            return Doc::new(blocks);
        };
        let segment = &toks[i..=j];
        let source = &html[toks[i].start..toks[j].end];
        let block = parse_block(segment, html)
            .filter(|b| {
                let mut out = String::new();
                write_block(b, &mut out);
                out == source
            })
            .unwrap_or_else(|| Block::kept(source, reason(segment)));
        push(&mut blocks, block, joined_next);
        i = j + 1;
    }
    if let Some(from) = loose_from {
        push(
            &mut blocks,
            Block::kept(&html[toks[from].start..], reason(&toks[from..])),
            true,
        );
    }
    Doc::new(blocks)
}

/// Turn a [`KeptReason::NotCanonical`] kept block at top-level `index` into editable blocks, rewriting its bytes
/// the way [`render`] writes them. Returns `None` when the block is not such a kept block.
#[must_use]
pub fn make_editable(doc: &Doc, index: usize) -> Option<Doc> {
    let Some(Block::Kept { attrs }) = doc.content.get(index) else {
        return None;
    };
    if attrs.reason != KeptReason::NotCanonical {
        return None;
    }
    let toks = tokenize(&attrs.html);
    let mut parsed = lenient_blocks(&attrs.html, &toks)?;
    if let Some(first) = parsed.first_mut() {
        first.set_joined(attrs.joined);
    }
    let mut content = doc.content.clone();
    content.splice(index..=index, parsed);
    Some(Doc::new(content).normalized())
}

/// Structure-only parse of palette HTML (no byte check): top-level blocks separated by whitespace.
fn lenient_blocks(html: &str, toks: &[Token]) -> Option<Vec<Block>> {
    if !toks.iter().any(|t| starts_block(&t.tok)) {
        let content = parse_inline(toks, html)?;
        return Some(vec![Block::paragraph(content)]);
    }
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if matches!(toks[i].tok, Tok::Text) && html[toks[i].start..toks[i].end].trim().is_empty() {
            i += 1;
            continue;
        }
        let j = matching_end(toks, i)?;
        blocks.push(parse_block(&toks[i..=j], html)?);
        i = j + 1;
    }
    Some(blocks)
}

fn push(blocks: &mut Vec<Block>, mut block: Block, joined: bool) {
    block.set_joined(joined && !blocks.is_empty());
    blocks.push(block);
}

/// A fragment with no block tags: one bare paragraph if it is canonical, else one kept block.
fn inline_only(html: &str, toks: &[Token]) -> Block {
    let bare = parse_inline(toks, html).map(|content| Block::Paragraph {
        attrs: ParagraphAttrs {
            joined: false,
            bare: true,
        },
        content,
    });
    match bare {
        Some(block) if render(&Doc::new(vec![block.clone()])) == html => block,
        _ => Block::kept(html, reason(toks)),
    }
}

fn reason(toks: &[Token]) -> KeptReason {
    let outside = toks.iter().any(|t| match &t.tok {
        Tok::Start { name, attrs, .. } => *attrs || !PALETTE.contains(&name.as_str()),
        Tok::End { name } => !PALETTE.contains(&name.as_str()),
        Tok::Text => false,
        Tok::Other => true,
    });
    if outside {
        KeptReason::OutsidePalette
    } else {
        KeptReason::NotCanonical
    }
}

fn starts_block(tok: &Tok) -> bool {
    matches!(tok, Tok::Start { name, .. } if !INLINE_TAGS.contains(&name.as_str()))
}

/// Index of the token closing the element opened at `i` (itself for void or self-closing tags).
fn matching_end(toks: &[Token], i: usize) -> Option<usize> {
    let Tok::Start {
        name, self_closing, ..
    } = &toks[i].tok
    else {
        return None;
    };
    if *self_closing || VOID_TAGS.contains(&name.as_str()) {
        return Some(i);
    }
    let mut depth = 0usize;
    for (j, t) in toks.iter().enumerate().skip(i) {
        match &t.tok {
            Tok::Start {
                name: n,
                self_closing: false,
                ..
            } if n == name => depth += 1,
            Tok::End { name: n } if n == name => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse one palette block (`seg` = its start tag ... its end tag). `None` when it is not palette HTML.
fn parse_block(seg: &[Token], html: &str) -> Option<Block> {
    let Tok::Start {
        name,
        attrs: false,
        self_closing: false,
    } = &seg.first()?.tok
    else {
        return None;
    };
    if seg.len() < 2 {
        return None;
    }
    let inner = &seg[1..seg.len() - 1];
    let block = match name.as_str() {
        "p" => Block::Paragraph {
            attrs: ParagraphAttrs::default(),
            content: parse_inline(inner, html)?,
        },
        "h3" | "h4" => Block::Heading {
            attrs: HeadingAttrs {
                joined: false,
                level: if name == "h4" { 4 } else { 3 },
            },
            content: parse_inline(inner, html)?,
        },
        "ul" | "ol" => {
            let items = children(inner, &["li"], html)?
                .into_iter()
                .map(|(_, c)| parse_inline(c, html).map(|content| ListItem { content }))
                .collect::<Option<Vec<_>>>()?;
            if items.is_empty() {
                return None; // the editor's schema holds no empty list
            }
            if name == "ul" {
                Block::BulletList {
                    attrs: BlockAttrs::default(),
                    content: items,
                }
            } else {
                Block::OrderedList {
                    attrs: BlockAttrs::default(),
                    content: items,
                }
            }
        }
        "table" => parse_table(inner, html)?,
        "blockquote" => Block::Blockquote {
            attrs: BlockAttrs::default(),
            content: parse_quote(inner, html)?,
        },
        _ => return None,
    };
    Some(block)
}

/// Direct children of an element, all of the given tag names. Whitespace between them is skipped here (the byte
/// check in [`parse`] keeps such a block as written; [`make_editable`] drops it).
fn children<'t>(
    inner: &'t [Token],
    names: &[&str],
    html: &str,
) -> Option<Vec<(String, &'t [Token])>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < inner.len() {
        if matches!(inner[i].tok, Tok::Text) && html[inner[i].start..inner[i].end].trim().is_empty()
        {
            i += 1;
            continue;
        }
        let Tok::Start {
            name,
            attrs: false,
            self_closing: false,
        } = &inner[i].tok
        else {
            return None;
        };
        if !names.contains(&name.as_str()) {
            return None;
        }
        let j = matching_end(inner, i)?;
        out.push((name.clone(), &inner[i + 1..j]));
        i = j + 1;
    }
    Some(out)
}

fn parse_table(inner: &[Token], html: &str) -> Option<Block> {
    let parts = children(inner, &["thead", "tbody", "tr"], html)?;
    let sections = parts.first().is_some_and(|(n, _)| n != "tr");
    let mut rows = Vec::new();
    if sections {
        let names: Vec<&str> = parts.iter().map(|(n, _)| n.as_str()).collect();
        if !matches!(names.as_slice(), ["thead"] | ["tbody"] | ["thead", "tbody"]) {
            return None;
        }
        for (name, part) in &parts {
            for (_, row) in children(part, &["tr"], html)? {
                rows.push(parse_row(row, html, name == "thead")?);
            }
        }
    } else {
        for (name, row) in &parts {
            if name != "tr" {
                return None;
            }
            rows.push(parse_row(row, html, false)?);
        }
    }
    if rows.is_empty() {
        return None; // the editor's schema holds no empty table
    }
    Some(Block::Table {
        attrs: TableAttrs {
            joined: false,
            sections,
        },
        content: rows,
    })
}

fn parse_row(inner: &[Token], html: &str, head: bool) -> Option<TableRow> {
    let cells = children(inner, &["td", "th"], html)?
        .into_iter()
        .map(|(name, c)| {
            let content = parse_inline(c, html)?;
            Some(if name == "th" {
                Cell::Header { content }
            } else {
                Cell::Data { content }
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(TableRow {
        attrs: RowAttrs { head },
        content: cells,
    })
}

/// Blockquote children: paragraphs, headings and lists separated by nothing or one `\n`.
fn parse_quote(inner: &[Token], html: &str) -> Option<Vec<Block>> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut joined = false;
    let mut i = 0;
    while i < inner.len() {
        if matches!(inner[i].tok, Tok::Text) {
            if &html[inner[i].start..inner[i].end] != "\n" || blocks.is_empty() || !joined {
                return None;
            }
            joined = false;
            i += 1;
            continue;
        }
        let Tok::Start { name, .. } = &inner[i].tok else {
            return None;
        };
        if !["p", "h3", "h4", "ul", "ol"].contains(&name.as_str()) {
            return None;
        }
        let j = matching_end(inner, i)?;
        let mut block = parse_block(&inner[i..=j], html)?;
        block.set_joined(joined);
        blocks.push(block);
        joined = true;
        i = j + 1;
    }
    if blocks.is_empty() || !joined {
        return None;
    }
    Some(blocks)
}

/// Inline content: text, `b`/`strong`/`i`/`em` and `br`. `None` for anything else or badly nested marks.
fn parse_inline(toks: &[Token], html: &str) -> Option<Vec<Inline>> {
    let mut stack: Vec<Mark> = Vec::new();
    let mut out: Vec<Inline> = Vec::new();
    for t in toks {
        match &t.tok {
            Tok::Text => {
                let text = entities::decode(&html[t.start..t.end]);
                out.push(Inline::Text {
                    text,
                    marks: stack.clone(),
                });
            }
            Tok::Start {
                name,
                attrs: false,
                self_closing,
            } => {
                if name == "br" {
                    out.push(Inline::HardBreak {
                        marks: stack.clone(),
                    });
                    continue;
                }
                let mark = mark_for(name)?;
                if *self_closing || stack.iter().any(|m| m.rank() == mark.rank()) {
                    return None;
                }
                stack.push(mark);
            }
            Tok::End { name } => {
                if stack.last()?.tag_name() != name {
                    return None;
                }
                stack.pop();
            }
            _ => return None,
        }
    }
    stack.is_empty().then_some(out)
}

fn mark_for(name: &str) -> Option<Mark> {
    Some(match name {
        "b" => Mark::bold(BoldTag::B),
        "strong" => Mark::bold(BoldTag::Strong),
        "i" => Mark::italic(ItalicTag::I),
        "em" => Mark::italic(ItalicTag::Em),
        _ => return None,
    })
}

/// Cut `html` into tags, text runs and other markup. Tokens cover the input without gaps.
fn tokenize(html: &str) -> Vec<Token> {
    let bytes = html.as_bytes();
    let mut toks: Vec<Token> = Vec::new();
    let mut text_from: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let tag = if bytes[i] == b'<' {
            read_tag(html, i)
        } else {
            None
        };
        match tag {
            Some((tok, end)) => {
                if let Some(from) = text_from.take() {
                    toks.push(Token {
                        tok: Tok::Text,
                        start: from,
                        end: i,
                    });
                }
                toks.push(Token { tok, start: i, end });
                i = end;
            }
            None => {
                text_from.get_or_insert(i);
                i += 1;
            }
        }
    }
    if let Some(from) = text_from {
        toks.push(Token {
            tok: Tok::Text,
            start: from,
            end: html.len(),
        });
    }
    toks
}

/// Read the markup starting at `html[i] == '<'`; `None` when the `<` is plain text.
fn read_tag(html: &str, i: usize) -> Option<(Tok, usize)> {
    let rest = &html[i..];
    if let Some(after) = rest.strip_prefix("<!--") {
        let end = after.find("-->").map_or(html.len(), |e| i + 4 + e + 3);
        return Some((Tok::Other, end));
    }
    let next = rest.as_bytes().get(1).copied()?;
    if next == b'!' || next == b'?' {
        let end = rest.find('>')?;
        return Some((Tok::Other, i + end + 1));
    }
    let closing = next == b'/';
    let name_from = if closing { 2 } else { 1 };
    if !rest.as_bytes().get(name_from)?.is_ascii_alphabetic() {
        return None;
    }
    let close = rest.find('>')?;
    let body = &rest[name_from..close];
    let name_len = body.bytes().take_while(u8::is_ascii_alphanumeric).count();
    let name = body[..name_len].to_ascii_lowercase();
    let tail = &body[name_len..];
    let tok = if closing {
        if !tail.trim().is_empty() {
            return Some((Tok::Other, i + close + 1));
        }
        Tok::End { name }
    } else {
        let trimmed = tail.trim_end();
        let self_closing = trimmed.ends_with('/');
        let attrs = !trimmed.trim_end_matches('/').trim().is_empty();
        Tok::Start {
            name,
            attrs,
            self_closing,
        }
    };
    Some((tok, i + close + 1))
}

#[cfg(test)]
mod tests {
    use super::super::render::write_inline;
    use super::*;

    fn inline_html(nodes: &[Inline]) -> String {
        let mut out = String::new();
        write_inline(nodes, &mut out);
        out
    }

    fn round_trip(html: &str) -> Doc {
        let doc = parse(html);
        assert_eq!(render(&doc), html, "render(parse) for {html:?}");
        doc
    }

    #[test]
    fn plain_palette_html_is_editable() {
        let doc = round_trip(
            "<p>a <b>b</b></p>\n<p><strong><em>N.</em> </strong>t</p><ul><li>x</li></ul>",
        );
        assert_eq!(doc.kept_count(), 0);
        assert!(!doc.content[1].joined());
        assert!(doc.content[2].joined());
    }

    #[test]
    fn outside_palette_is_kept_byte_for_byte() {
        let html = "<p>a</p>\n<pre>  x\n y</pre>\n<p class=\"table-text\">t</p><h5>c</h5>";
        let doc = round_trip(html);
        let kept: Vec<_> = doc
            .content
            .iter()
            .filter_map(|b| match b {
                Block::Kept { attrs } => Some((attrs.html.as_str(), attrs.reason)),
                _ => None,
            })
            .collect();
        assert_eq!(
            kept,
            vec![
                ("<pre>  x\n y</pre>", KeptReason::OutsidePalette),
                ("<p class=\"table-text\">t</p>", KeptReason::OutsidePalette),
                ("<h5>c</h5>", KeptReason::OutsidePalette),
            ]
        );
    }

    #[test]
    fn non_canonical_palette_html_is_kept_and_can_be_made_editable() {
        for html in [
            "<p>it&#8217;s</p>",
            "<p>a<br>b</p>",
            "<p><em><strong>x</strong></em></p>",
            "<p><b>a</b><b>b</b></p>",
            "<p><b></b></p>",
            "<P>x</P>",
            "<ul>\n<li>x</li>\n</ul>",
        ] {
            let doc = round_trip(html);
            assert_eq!(doc.content.len(), 1, "{html}");
            let Block::Kept { attrs } = &doc.content[0] else {
                panic!("{html} not kept")
            };
            assert_eq!(attrs.reason, KeptReason::NotCanonical, "{html}");
            let editable = make_editable(&doc, 0).unwrap_or_else(|| panic!("{html}"));
            assert_eq!(editable.kept_count(), 0, "{html}");
        }
        let doc = parse("<p>it&#8217;s</p>");
        assert_eq!(
            render(&make_editable(&doc, 0).unwrap()),
            "<p>it\u{2019}s</p>"
        );
    }

    #[test]
    fn plain_text_is_a_bare_paragraph() {
        let doc = round_trip("Acolytes are junior members of a clergy &amp; more.");
        assert_eq!(
            doc.content,
            vec![Block::Paragraph {
                attrs: ParagraphAttrs {
                    joined: false,
                    bare: true
                },
                content: vec![Inline::text(
                    "Acolytes are junior members of a clergy & more."
                )],
            }]
        );
        assert_eq!(round_trip("").content, vec![]);
        assert_eq!(round_trip("a < b").kept_count(), 1);
        assert_eq!(round_trip("x <span>y</span>").kept_count(), 1);
    }

    #[test]
    fn odd_gaps_and_broken_html_are_kept() {
        for html in [
            "\n<p>a</p>",
            "<p>a</p>\n",
            "<p>a</p>  <p>b</p>",
            "<p>a</p>\n\n<p>b</p>",
            "text<p>a</p>",
            "<p>a</p></p>",
            "<p>unclosed",
            "<p>a</p><!-- c --><p>b</p>",
            "<table><tbody><tr><td>1</td></tr></tbody><thead></thead></table>",
            "<hr><p>x</p>",
            "<p>a <b>b</p>",
            "<ul></ul>",
            "<table></table>",
            "<table><thead></thead></table>",
        ] {
            let doc = round_trip(html);
            assert!(doc.kept_count() > 0, "{html}");
        }
    }

    #[test]
    fn tables_record_their_shape() {
        let doc = round_trip("<table><tr><th>A</th></tr><tr><td>1</td></tr></table>");
        let Block::Table { attrs, content } = &doc.content[0] else {
            panic!("table")
        };
        assert!(!attrs.sections);
        assert!(content.iter().all(|r| !r.attrs.head));
        let doc = round_trip(
            "<table><thead><tr><th>A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        );
        let Block::Table { attrs, content } = &doc.content[0] else {
            panic!("table")
        };
        assert!(attrs.sections);
        assert!(content[0].attrs.head && !content[1].attrs.head);
        round_trip("<table><tbody><tr><td>1</td></tr></tbody></table>");
    }

    #[test]
    fn blockquote_holds_blocks() {
        let doc = round_trip("<blockquote><p>a</p>\n<ul><li>b</li></ul></blockquote>");
        assert_eq!(doc.kept_count(), 0);
        assert_eq!(
            round_trip("<blockquote>\n<p>a</p></blockquote>").kept_count(),
            1
        );
        assert_eq!(round_trip("<blockquote></blockquote>").kept_count(), 1);
    }

    #[test]
    fn inline_parse_records_marks_outermost_first() {
        let toks = tokenize("<strong><em>A</em> </strong>b<br />c");
        let nodes = parse_inline(&toks, "<strong><em>A</em> </strong>b<br />c").unwrap();
        assert_eq!(nodes[0].marks().len(), 2);
        assert_eq!(nodes[0].marks()[0].tag_name(), "strong");
        assert_eq!(inline_html(&nodes), "<strong><em>A</em> </strong>b<br />c");
    }

    #[test]
    fn tokenizer_treats_lone_angle_brackets_as_text() {
        let toks = tokenize("a < b <3 </ x");
        assert_eq!(toks.len(), 1);
        assert!(matches!(toks[0].tok, Tok::Text));
    }
}
