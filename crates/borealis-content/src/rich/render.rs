//! The one writer: [`Doc`] to canonical HTML.
//!
//! * Text is written as literal Unicode. Only `&`, `<` and `>` are escaped (`&amp;`, `&lt;`, `&gt;`); no numeric
//!   or other named references, no backslashes.
//! * Blocks are separated by `\n` unless a block is `joined`. List items, rows and cells are written without
//!   separators (as the bundled SRD files are).
//! * Marks open outermost-first (bold, then italic) and are closed in reverse order; a mark stays open across
//!   adjacent nodes that keep it.
//! * Kept blocks are written back unchanged.

use super::{Block, Cell, Doc, Inline, Mark, TableRow};

/// Write `doc` as an HTML fragment.
#[must_use]
pub fn render(doc: &Doc) -> String {
    let mut out = String::new();
    write_blocks(&doc.content, &mut out);
    out
}

pub(super) fn write_blocks(blocks: &[Block], out: &mut String) {
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 && !block.joined() {
            out.push('\n');
        }
        write_block(block, out);
    }
}

pub(super) fn write_block(block: &Block, out: &mut String) {
    match block {
        Block::Paragraph { attrs, content } => {
            if attrs.bare {
                write_inline(content, out);
            } else {
                wrap(out, "p", |o| write_inline(content, o));
            }
        }
        Block::Heading { attrs, content } => {
            let tag = if attrs.level >= 4 { "h4" } else { "h3" };
            wrap(out, tag, |o| write_inline(content, o));
        }
        Block::BulletList { content, .. } => wrap(out, "ul", |o| write_items(content, o)),
        Block::OrderedList { content, .. } => wrap(out, "ol", |o| write_items(content, o)),
        Block::Table { attrs, content } => wrap(out, "table", |o| {
            if attrs.sections {
                let head = content.iter().take_while(|r| r.attrs.head).count();
                let (head_rows, body_rows) = content.split_at(head);
                if !head_rows.is_empty() {
                    wrap(o, "thead", |o| write_rows(head_rows, o));
                }
                if !body_rows.is_empty() {
                    wrap(o, "tbody", |o| write_rows(body_rows, o));
                }
            } else {
                write_rows(content, o);
            }
        }),
        Block::Blockquote { content, .. } => wrap(out, "blockquote", |o| write_blocks(content, o)),
        Block::Kept { attrs } => out.push_str(&attrs.html),
    }
}

fn write_items(items: &[super::ListItem], out: &mut String) {
    for item in items {
        wrap(out, "li", |o| write_inline(&item.content, o));
    }
}

fn write_rows(rows: &[TableRow], out: &mut String) {
    for row in rows {
        wrap(out, "tr", |o| {
            for cell in &row.content {
                match cell {
                    Cell::Data { content } => wrap(o, "td", |o| write_inline(content, o)),
                    Cell::Header { content } => wrap(o, "th", |o| write_inline(content, o)),
                }
            }
        });
    }
}

fn wrap(out: &mut String, tag: &str, body: impl FnOnce(&mut String)) {
    out.push('<');
    out.push_str(tag);
    out.push('>');
    body(out);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

/// Write inline nodes, opening and closing marks as the nodes need them.
pub(super) fn write_inline(nodes: &[Inline], out: &mut String) {
    let mut open: Vec<&Mark> = Vec::new();
    for node in nodes {
        let mut wanted: Vec<&Mark> = node.marks().iter().collect();
        wanted.sort_by_key(|m| m.rank());
        let keep = open.iter().zip(&wanted).take_while(|(a, b)| a == b).count();
        while open.len() > keep {
            if let Some(mark) = open.pop() {
                close_tag(mark, out);
            }
        }
        for mark in &wanted[keep..] {
            out.push('<');
            out.push_str(mark.tag_name());
            out.push('>');
            open.push(mark);
        }
        match node {
            Inline::Text { text, .. } => escape_text(text, out),
            Inline::HardBreak { .. } => out.push_str("<br />"),
        }
    }
    while let Some(mark) = open.pop() {
        close_tag(mark, out);
    }
}

fn close_tag(mark: &Mark, out: &mut String) {
    out.push_str("</");
    out.push_str(mark.tag_name());
    out.push('>');
}

/// Escape the minimum HTML text needs.
pub(super) fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        BlockAttrs, BoldTag, HeadingAttrs, ItalicTag, KeptReason, ListItem, ParagraphAttrs,
        RowAttrs, TableAttrs,
    };
    use super::*;

    fn t(text: &str, marks: Vec<Mark>) -> Inline {
        Inline::Text {
            text: text.into(),
            marks,
        }
    }

    #[test]
    fn escapes_only_the_minimum() {
        let doc = Doc::new(vec![Block::paragraph(vec![Inline::text(
            "Tom & Jerry <3 > \u{201c}quotes\u{201d} \u{2019} \u{e9} \\ \u{a0}",
        )])]);
        assert_eq!(
            render(&doc),
            "<p>Tom &amp; Jerry &lt;3 &gt; \u{201c}quotes\u{201d} \u{2019} \u{e9} \\ \u{a0}</p>"
        );
    }

    #[test]
    fn blocks_are_separated_unless_joined() {
        let mut second = Block::paragraph(vec![Inline::text("b")]);
        let third = Block::paragraph(vec![Inline::text("c")]);
        second.set_joined(true);
        let doc = Doc::new(vec![
            Block::paragraph(vec![Inline::text("a")]),
            second,
            third,
        ]);
        assert_eq!(render(&doc), "<p>a</p><p>b</p>\n<p>c</p>");
    }

    #[test]
    fn marks_nest_bold_outside_italic_and_stay_open() {
        let bold = Mark::bold(BoldTag::Strong);
        let italic = Mark::italic(ItalicTag::Em);
        let doc = Doc::new(vec![Block::paragraph(vec![
            t("Name.", vec![italic.clone(), bold.clone()]),
            t(" ", vec![bold.clone()]),
            t("Body ", vec![]),
            t("x", vec![italic]),
            Inline::HardBreak { marks: vec![] },
            t("y", vec![bold]),
        ])]);
        assert_eq!(
            render(&doc),
            "<p><strong><em>Name.</em> </strong>Body <em>x</em><br /><strong>y</strong></p>"
        );
    }

    #[test]
    fn different_spellings_are_different_marks() {
        let doc = Doc::new(vec![Block::paragraph(vec![
            t("a", vec![Mark::bold(BoldTag::B)]),
            t("b", vec![Mark::bold(BoldTag::Strong)]),
        ])]);
        assert_eq!(render(&doc), "<p><b>a</b><strong>b</strong></p>");
    }

    #[test]
    fn writes_every_block_kind() {
        let row = |head: bool, cells: Vec<Cell>| TableRow {
            attrs: RowAttrs { head },
            content: cells,
        };
        let td = |s: &str| Cell::Data {
            content: vec![Inline::text(s)],
        };
        let th = |s: &str| Cell::Header {
            content: vec![Inline::text(s)],
        };
        let doc = Doc::new(vec![
            Block::Heading {
                attrs: HeadingAttrs {
                    joined: false,
                    level: 4,
                },
                content: vec![Inline::text("H")],
            },
            Block::BulletList {
                attrs: BlockAttrs::default(),
                content: vec![
                    ListItem {
                        content: vec![Inline::text("1")],
                    },
                    ListItem::default(),
                ],
            },
            Block::OrderedList {
                attrs: BlockAttrs::default(),
                content: vec![ListItem::default()],
            },
            Block::Table {
                attrs: TableAttrs::default(),
                content: vec![row(true, vec![th("A")]), row(false, vec![td("1")])],
            },
            Block::Table {
                attrs: TableAttrs {
                    joined: false,
                    sections: false,
                },
                content: vec![row(false, vec![th("A"), td("1")])],
            },
            Block::Blockquote {
                attrs: BlockAttrs::default(),
                content: vec![Block::paragraph(vec![Inline::text("q")])],
            },
            Block::kept("<pre> x</pre>", KeptReason::OutsidePalette),
        ]);
        assert_eq!(
            render(&doc),
            "<h4>H</h4>\n<ul><li>1</li><li></li></ul>\n<ol><li></li></ol>\n\
             <table><thead><tr><th>A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>\n\
             <table><tr><th>A</th><td>1</td></tr></table>\n<blockquote><p>q</p></blockquote>\n<pre> x</pre>"
        );
    }

    #[test]
    fn bare_paragraph_has_no_wrapper() {
        let doc = Doc::new(vec![Block::Paragraph {
            attrs: ParagraphAttrs {
                joined: false,
                bare: true,
            },
            content: vec![Inline::text("Plain & simple")],
        }]);
        assert_eq!(render(&doc), "Plain &amp; simple");
        assert_eq!(render(&Doc::default()), "");
    }
}
