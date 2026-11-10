//! Normalization of trees that come from the editor, and of plain text fields.
//!
//! Applied at the boundary (the editor's document, a typed name), never to parsed bundled text, so an untouched
//! description keeps its bytes. It is the identity on every parsed bundled description (tested over the corpus).

use unicode_normalization::UnicodeNormalization;

use super::{Block, Cell, Doc, Inline, Mark};

const NBSP: char = '\u{a0}';

impl Doc {
    /// The canonical form of an editor document:
    /// * text NFC, no-break spaces become spaces (except in table cells), control characters other than tab and
    ///   line feed removed; empty text dropped, adjacent text with the same marks merged;
    /// * marks sorted (bold outside italic), one of each kind;
    /// * the first block of every block list is not `joined`; `bare` only on a lone paragraph; heading levels 3/4;
    ///   head rows only as the leading rows of a table with sections;
    /// * lists, tables and quotes without content are dropped; a document holding one empty paragraph is empty.
    #[must_use]
    pub fn normalized(&self) -> Doc {
        let mut blocks: Vec<Block> = self
            .content
            .iter()
            .filter(|b| !is_empty_container(b))
            .map(|b| normalize_block(b, true))
            .collect();
        if blocks.len() > 1 {
            for block in &mut blocks {
                if let Block::Paragraph { attrs, .. } = block {
                    attrs.bare = false;
                }
            }
        }
        if let [Block::Paragraph { content, .. }] = blocks.as_slice() {
            if content.is_empty() {
                blocks.clear();
            }
        }
        fix_first_joined(&mut blocks);
        Doc::new(blocks)
    }
}

/// Normalize a plain text field (names, short texts): NFC, control characters removed, every run of white space
/// (tabs, line breaks and no-break spaces included) one space, trimmed.
#[must_use]
pub fn normalize_plain_text(text: &str) -> String {
    let cleaned: String = text
        .nfc()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A list, table or quote with nothing in it (the editor's schema cannot hold one).
fn is_empty_container(block: &Block) -> bool {
    match block {
        Block::BulletList { content, .. } | Block::OrderedList { content, .. } => {
            content.is_empty()
        }
        Block::Table { content, .. } => content.is_empty(),
        Block::Blockquote { content, .. } => content.iter().all(is_empty_container),
        _ => false,
    }
}

fn fix_first_joined(blocks: &mut [Block]) {
    if let Some(first) = blocks.first_mut() {
        first.set_joined(false);
    }
}

fn normalize_block(block: &Block, top: bool) -> Block {
    match block {
        Block::Paragraph { attrs, content } => {
            let mut attrs = attrs.clone();
            attrs.bare &= top;
            Block::Paragraph {
                attrs,
                content: normalize_inline(content, false),
            }
        }
        Block::Heading { attrs, content } => {
            let mut attrs = attrs.clone();
            attrs.level = if attrs.level >= 4 { 4 } else { 3 };
            Block::Heading {
                attrs,
                content: normalize_inline(content, false),
            }
        }
        Block::BulletList { attrs, content } => Block::BulletList {
            attrs: attrs.clone(),
            content: normalize_items(content),
        },
        Block::OrderedList { attrs, content } => Block::OrderedList {
            attrs: attrs.clone(),
            content: normalize_items(content),
        },
        Block::Table { attrs, content } => {
            let mut in_head = attrs.sections;
            let rows = content
                .iter()
                .map(|row| {
                    in_head &= row.attrs.head;
                    let mut row = row.clone();
                    row.attrs.head = in_head;
                    row.content = row
                        .content
                        .iter()
                        .map(|cell| match cell {
                            Cell::Data { content } => Cell::Data {
                                content: normalize_inline(content, true),
                            },
                            Cell::Header { content } => Cell::Header {
                                content: normalize_inline(content, true),
                            },
                        })
                        .collect();
                    row
                })
                .collect();
            Block::Table {
                attrs: attrs.clone(),
                content: rows,
            }
        }
        Block::Blockquote { attrs, content } => {
            let mut children: Vec<Block> = content
                .iter()
                .filter(|b| !is_empty_container(b))
                .map(|b| normalize_block(b, false))
                .collect();
            fix_first_joined(&mut children);
            Block::Blockquote {
                attrs: attrs.clone(),
                content: children,
            }
        }
        Block::Kept { .. } => block.clone(),
    }
}

fn normalize_items(items: &[super::ListItem]) -> Vec<super::ListItem> {
    items
        .iter()
        .map(|item| super::ListItem {
            content: normalize_inline(&item.content, false),
        })
        .collect()
}

fn normalize_marks(marks: &[Mark]) -> Vec<Mark> {
    let mut out: Vec<Mark> = Vec::with_capacity(marks.len());
    for mark in marks {
        if !out.iter().any(|m| m.rank() == mark.rank()) {
            out.push(mark.clone());
        }
    }
    out.sort_by_key(Mark::rank);
    out
}

fn normalize_chars(text: &str, in_cell: bool) -> String {
    text.nfc()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .map(|c| if c == NBSP && !in_cell { ' ' } else { c })
        .collect()
}

fn normalize_inline(nodes: &[Inline], in_cell: bool) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Inline::Text { text, marks } => {
                let text = normalize_chars(text, in_cell);
                if text.is_empty() {
                    continue;
                }
                let marks = normalize_marks(marks);
                if let Some(Inline::Text {
                    text: prev,
                    marks: prev_marks,
                }) = out.last_mut()
                {
                    if *prev_marks == marks {
                        prev.push_str(&text);
                        continue;
                    }
                }
                out.push(Inline::Text { text, marks });
            }
            Inline::HardBreak { marks } => out.push(Inline::HardBreak {
                marks: normalize_marks(marks),
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{
        BlockAttrs, BoldTag, HeadingAttrs, ItalicTag, KeptReason, ListItem, ParagraphAttrs,
        RowAttrs, TableAttrs, TableRow,
    };
    use super::*;

    fn text(t: &str, marks: Vec<Mark>) -> Inline {
        Inline::Text {
            text: t.into(),
            marks,
        }
    }

    #[test]
    fn text_is_nfc_without_nbsp_or_controls() {
        let doc = Doc::new(vec![Block::paragraph(vec![text(
            "Cafe\u{301}\u{a0}ok\u{7}\u{0}\tx",
            vec![],
        )])]);
        assert_eq!(
            doc.normalized().content,
            vec![Block::paragraph(vec![text("Caf\u{e9} ok\tx", vec![])])]
        );
    }

    #[test]
    fn nbsp_stays_in_table_cells() {
        let doc = Doc::new(vec![Block::Table {
            attrs: TableAttrs::default(),
            content: vec![TableRow {
                attrs: RowAttrs { head: false },
                content: vec![Cell::Data {
                    content: vec![text("1\u{a0}000", vec![])],
                }],
            }],
        }]);
        assert_eq!(doc.normalized(), doc);
    }

    #[test]
    fn marks_are_sorted_deduplicated_and_text_merged() {
        let b = Mark::bold(BoldTag::B);
        let i = Mark::italic(ItalicTag::Em);
        let doc = Doc::new(vec![Block::paragraph(vec![
            text("a", vec![i.clone(), b.clone(), Mark::bold(BoldTag::Strong)]),
            text("b", vec![b.clone(), i.clone()]),
            text("", vec![]),
            text("c", vec![]),
        ])]);
        assert_eq!(
            doc.normalized().content,
            vec![Block::paragraph(vec![
                text("ab", vec![b, i]),
                text("c", vec![])
            ])]
        );
    }

    #[test]
    fn block_rules() {
        let mut joined = Block::paragraph(vec![text("a", vec![])]);
        joined.set_joined(true);
        let bare = Block::Paragraph {
            attrs: ParagraphAttrs {
                joined: false,
                bare: true,
            },
            content: vec![text("b", vec![])],
        };
        let doc = Doc::new(vec![
            joined,
            bare,
            Block::Heading {
                attrs: HeadingAttrs {
                    joined: false,
                    level: 6,
                },
                content: vec![],
            },
            Block::Table {
                attrs: TableAttrs {
                    joined: false,
                    sections: false,
                },
                content: vec![TableRow {
                    attrs: RowAttrs { head: true },
                    content: vec![],
                }],
            },
            Block::Table {
                attrs: TableAttrs::default(),
                content: vec![
                    TableRow {
                        attrs: RowAttrs { head: false },
                        content: vec![],
                    },
                    TableRow {
                        attrs: RowAttrs { head: true },
                        content: vec![],
                    },
                ],
            },
            Block::BulletList {
                attrs: BlockAttrs::default(),
                content: vec![ListItem::default()],
            },
            Block::OrderedList {
                attrs: BlockAttrs::default(),
                content: vec![],
            },
            Block::Blockquote {
                attrs: BlockAttrs::default(),
                content: vec![],
            },
            Block::Table {
                attrs: TableAttrs::default(),
                content: vec![],
            },
            Block::kept("<pre>\u{a0}</pre>", KeptReason::OutsidePalette),
        ])
        .normalized();
        assert!(!doc.content[0].joined());
        assert!(matches!(&doc.content[1], Block::Paragraph { attrs, .. } if !attrs.bare));
        assert!(matches!(&doc.content[2], Block::Heading { attrs, .. } if attrs.level == 4));
        for table in &doc.content[3..5] {
            let Block::Table { content, .. } = table else {
                panic!("table")
            };
            assert!(content.iter().all(|r| !r.attrs.head));
        }
        assert_eq!(doc.content.len(), 7, "empty list, quote and table dropped");
        assert_eq!(
            doc.content[6],
            Block::kept("<pre>\u{a0}</pre>", KeptReason::OutsidePalette)
        );
    }

    #[test]
    fn one_empty_paragraph_is_an_empty_document() {
        assert_eq!(
            Doc::new(vec![Block::paragraph(vec![text("", vec![])])]).normalized(),
            Doc::default()
        );
        let lone_bare = Doc::new(vec![Block::Paragraph {
            attrs: ParagraphAttrs {
                joined: false,
                bare: true,
            },
            content: vec![text("x", vec![])],
        }]);
        assert_eq!(lone_bare.normalized(), lone_bare);
    }

    #[test]
    fn plain_text_fields() {
        assert_eq!(
            normalize_plain_text("  Fire\u{a0}bo\u{7}lt\n\t of  doom "),
            "Fire bolt of doom"
        );
        assert_eq!(normalize_plain_text("E\u{301}lan"), "\u{c9}lan");
    }
}
