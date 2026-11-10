//! Rich text for element descriptions (homebrew creator, spec section 7).
//!
//! A description is stored as an HTML fragment string ([`crate::Element::description`]). The creator edits it as a
//! [`Doc`] tree and [`render`] is the **one writer** that turns a tree back into HTML, so a stray escape cannot
//! exist: text is written as literal Unicode and only `&`, `<` and `>` are escaped.
//!
//! * [`parse`] reads any HTML fragment. Blocks inside the editor's palette become editable nodes; everything else
//!   (other tags, attributes, or palette tags written differently than [`render`] writes them) becomes a
//!   [`Block::Kept`] block holding the original bytes. `render(&parse(html)) == html` for every input.
//! * [`Doc::normalized`] is applied to trees that come from the editor (NFC, no-break spaces, control characters).
//!   `parse(&render(&doc)) == doc` for every normalized tree whose kept blocks each hold one complete element.
//! * [`lint`] / [`fix`] find and repair text that looks like an escape artefact from pasting (`\n`, `&amp;amp;`,
//!   `â€™`, stray backslashes).
//!
//! The JSON form of the tree is the ProseMirror JSON of the UI's schema mirror
//! (`ui/src/pages/homebrew/rich/schema.ts`): node `type` names, `attrs` always written in full, `content` and
//! `marks` left out when empty. Shared fixtures (`tests/fixtures/rich_text.json`) keep both sides in step.
//!
//! Planned home: `borealis-authoring::rich` (spec 15.1) once that crate exists; it lives here until then.

mod entities;
mod lint;
mod normalize;
mod parse;
mod render;

use serde::{Deserialize, Serialize};

pub use lint::{fix, fix_text, lint, lint_text, EscapeFinding, EscapeKind, TextFinding};
pub use normalize::normalize_plain_text;
pub use parse::{make_editable, parse};
pub use render::render;

/// A description document: a list of blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "doc", from = "DocRepr")]
pub struct Doc {
    /// Top-level blocks, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Block>,
}

/// A block-level node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// `<p>`, or bare inline text when the whole description is unwrapped text.
    Paragraph {
        /// Layout attributes.
        #[serde(default)]
        attrs: ParagraphAttrs,
        /// Inline content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<Inline>,
    },
    /// `<h3>` (level 3, "Heading") or `<h4>` (level 4, "Subheading").
    Heading {
        /// Level and layout.
        #[serde(default)]
        attrs: HeadingAttrs,
        /// Inline content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<Inline>,
    },
    /// `<ul>`.
    BulletList {
        /// Layout attributes.
        #[serde(default)]
        attrs: BlockAttrs,
        /// The items.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<ListItem>,
    },
    /// `<ol>`.
    OrderedList {
        /// Layout attributes.
        #[serde(default)]
        attrs: BlockAttrs,
        /// The items.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<ListItem>,
    },
    /// `<table>`.
    Table {
        /// Shape and layout.
        #[serde(default)]
        attrs: TableAttrs,
        /// The rows.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<TableRow>,
    },
    /// `<blockquote>` holding paragraphs, headings and lists.
    Blockquote {
        /// Layout attributes.
        #[serde(default)]
        attrs: BlockAttrs,
        /// Child blocks.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<Block>,
    },
    /// Formatting outside the editor's set, kept byte-for-byte: shown rendered, read-only, removable.
    Kept {
        /// The original bytes and why they were kept.
        #[serde(default)]
        attrs: KeptAttrs,
    },
}

/// Attributes every block has.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockAttrs {
    /// Written directly after the previous block. `false` (the default) puts a line break (`\n`) between them.
    pub joined: bool,
}

/// Paragraph attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParagraphAttrs {
    /// See [`BlockAttrs::joined`].
    pub joined: bool,
    /// Written without `<p>`: a description that is plain text. Only valid as the only block.
    pub bare: bool,
}

/// Heading attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeadingAttrs {
    /// See [`BlockAttrs::joined`].
    pub joined: bool,
    /// 3 (`<h3>`, "Heading") or 4 (`<h4>`, "Subheading").
    pub level: u8,
}

impl Default for HeadingAttrs {
    fn default() -> Self {
        Self {
            joined: false,
            level: 3,
        }
    }
}

/// Table attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableAttrs {
    /// See [`BlockAttrs::joined`].
    pub joined: bool,
    /// Rows are written inside `<thead>` (rows with `head`) and `<tbody>`; `false` writes `<tr>` directly.
    pub sections: bool,
}

impl Default for TableAttrs {
    fn default() -> Self {
        Self {
            joined: false,
            sections: true,
        }
    }
}

/// Why a block was kept as written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeptReason {
    /// Tags or attributes outside the editor's set (`h5`, `pre`, `class="..."`, ...).
    #[default]
    OutsidePalette,
    /// Only palette tags, written differently than Borealis writes them (entities, `<br>`, mark order...).
    /// [`make_editable`] can turn it into editable blocks (rewriting its bytes).
    NotCanonical,
}

/// Attributes of a [`Block::Kept`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeptAttrs {
    /// See [`BlockAttrs::joined`].
    pub joined: bool,
    /// The original HTML, written back unchanged.
    pub html: String,
    /// Why it was kept.
    pub reason: KeptReason,
}

/// `<li>`: inline content only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "list_item", from = "ListItemRepr")]
pub struct ListItem {
    /// Inline content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Inline>,
}

/// Row attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RowAttrs {
    /// In `<thead>` (tables with `sections` only; head rows come first).
    pub head: bool,
}

/// `<tr>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "table_row", from = "TableRowRepr")]
pub struct TableRow {
    /// Row attributes.
    #[serde(default)]
    pub attrs: RowAttrs,
    /// The cells.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Cell>,
}

/// `<td>` or `<th>`: inline content only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Cell {
    /// `<td>`.
    #[serde(rename = "table_cell")]
    Data {
        /// Inline content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<Inline>,
    },
    /// `<th>`.
    #[serde(rename = "table_header")]
    Header {
        /// Inline content.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<Inline>,
    },
}

/// An inline node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    /// Text (literal characters, never escaped in the tree).
    Text {
        /// The characters.
        text: String,
        /// Marks, outermost first (bold before italic).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<Mark>,
    },
    /// `<br />`.
    HardBreak {
        /// Marks the break sits inside.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<Mark>,
    },
}

/// An inline mark. The tag spelling is recorded so bundled text is written back byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mark {
    /// `<b>` or `<strong>`.
    Bold {
        /// Tag spelling.
        #[serde(default)]
        attrs: BoldAttrs,
    },
    /// `<i>` or `<em>`.
    Italic {
        /// Tag spelling.
        #[serde(default)]
        attrs: ItalicAttrs,
    },
}

/// Bold tag spelling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoldTag {
    /// `<b>` (the default for new text, as SRD 5.2.1 writes it).
    #[default]
    B,
    /// `<strong>` (SRD 5.1).
    Strong,
}

/// Italic tag spelling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItalicTag {
    /// `<i>` (the default for new text).
    #[default]
    I,
    /// `<em>` (SRD 5.1).
    Em,
}

/// Bold mark attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoldAttrs {
    /// Tag spelling.
    pub tag: BoldTag,
}

/// Italic mark attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItalicAttrs {
    /// Tag spelling.
    pub tag: ItalicTag,
}

// serde does not check the tag of an internally tagged struct; these one-variant enums do.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DocRepr {
    Doc {
        #[serde(default)]
        content: Vec<Block>,
    },
}

impl From<DocRepr> for Doc {
    fn from(DocRepr::Doc { content }: DocRepr) -> Self {
        Self { content }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ListItemRepr {
    ListItem {
        #[serde(default)]
        content: Vec<Inline>,
    },
}

impl From<ListItemRepr> for ListItem {
    fn from(ListItemRepr::ListItem { content }: ListItemRepr) -> Self {
        Self { content }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TableRowRepr {
    TableRow {
        #[serde(default)]
        attrs: RowAttrs,
        #[serde(default)]
        content: Vec<Cell>,
    },
}

impl From<TableRowRepr> for TableRow {
    fn from(TableRowRepr::TableRow { attrs, content }: TableRowRepr) -> Self {
        Self { attrs, content }
    }
}

impl Mark {
    /// Bold with the given spelling.
    #[must_use]
    pub fn bold(tag: BoldTag) -> Self {
        Self::Bold {
            attrs: BoldAttrs { tag },
        }
    }

    /// Italic with the given spelling.
    #[must_use]
    pub fn italic(tag: ItalicTag) -> Self {
        Self::Italic {
            attrs: ItalicAttrs { tag },
        }
    }

    /// Nesting rank: lower ranks are written outside higher ones.
    #[must_use]
    pub fn rank(&self) -> u8 {
        match self {
            Self::Bold { .. } => 0,
            Self::Italic { .. } => 1,
        }
    }

    /// The HTML tag name.
    #[must_use]
    pub fn tag_name(&self) -> &'static str {
        match self {
            Self::Bold { attrs } => match attrs.tag {
                BoldTag::B => "b",
                BoldTag::Strong => "strong",
            },
            Self::Italic { attrs } => match attrs.tag {
                ItalicTag::I => "i",
                ItalicTag::Em => "em",
            },
        }
    }
}

impl Inline {
    /// Plain text node.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            marks: Vec::new(),
        }
    }

    /// The node's marks.
    #[must_use]
    pub fn marks(&self) -> &[Mark] {
        match self {
            Self::Text { marks, .. } | Self::HardBreak { marks } => marks,
        }
    }
}

impl Block {
    /// A `<p>` with the given inline content.
    #[must_use]
    pub fn paragraph(content: Vec<Inline>) -> Self {
        Self::Paragraph {
            attrs: ParagraphAttrs::default(),
            content,
        }
    }

    /// A kept block.
    #[must_use]
    pub fn kept(html: impl Into<String>, reason: KeptReason) -> Self {
        Self::Kept {
            attrs: KeptAttrs {
                joined: false,
                html: html.into(),
                reason,
            },
        }
    }

    /// Whether the block is written directly after the previous one.
    #[must_use]
    pub fn joined(&self) -> bool {
        match self {
            Self::Paragraph { attrs, .. } => attrs.joined,
            Self::Heading { attrs, .. } => attrs.joined,
            Self::BulletList { attrs, .. }
            | Self::OrderedList { attrs, .. }
            | Self::Blockquote { attrs, .. } => attrs.joined,
            Self::Table { attrs, .. } => attrs.joined,
            Self::Kept { attrs } => attrs.joined,
        }
    }

    /// Set [`Block::joined`].
    pub fn set_joined(&mut self, joined: bool) {
        match self {
            Self::Paragraph { attrs, .. } => attrs.joined = joined,
            Self::Heading { attrs, .. } => attrs.joined = joined,
            Self::BulletList { attrs, .. }
            | Self::OrderedList { attrs, .. }
            | Self::Blockquote { attrs, .. } => {
                attrs.joined = joined;
            }
            Self::Table { attrs, .. } => attrs.joined = joined,
            Self::Kept { attrs } => attrs.joined = joined,
        }
    }

    /// Whether this is a kept block.
    #[must_use]
    pub fn is_kept(&self) -> bool {
        matches!(self, Self::Kept { .. })
    }
}

impl Doc {
    /// A document of the given blocks.
    #[must_use]
    pub fn new(content: Vec<Block>) -> Self {
        Self { content }
    }

    /// Number of kept blocks (top level).
    #[must_use]
    pub fn kept_count(&self) -> usize {
        self.content.iter().filter(|b| b.is_kept()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_shape_matches_prosemirror() {
        let doc = Doc::new(vec![Block::paragraph(vec![
            Inline::Text {
                text: "Hot".into(),
                marks: vec![Mark::bold(BoldTag::B)],
            },
            Inline::HardBreak { marks: vec![] },
        ])]);
        let value = serde_json::to_value(&doc).unwrap();
        assert_eq!(
            value,
            json!({"type": "doc", "content": [{"type": "paragraph", "attrs": {"joined": false, "bare": false},
                "content": [{"type": "text", "text": "Hot", "marks": [{"type": "bold", "attrs": {"tag": "b"}}]},
                            {"type": "hard_break"}]}]})
        );
        let back: Doc = serde_json::from_value(value).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn missing_attrs_take_defaults() {
        let doc: Doc = serde_json::from_value(json!({"type": "doc", "content": [
            {"type": "heading", "content": [{"type": "text", "text": "A"}]},
            {"type": "table", "content": [{"type": "table_row", "content": [{"type": "table_header"}]}]}
        ]}))
        .unwrap();
        assert_eq!(
            doc.content[0],
            Block::Heading {
                attrs: HeadingAttrs::default(),
                content: vec![Inline::text("A")]
            }
        );
        let Block::Table { attrs, content } = &doc.content[1] else {
            panic!("table")
        };
        assert!(attrs.sections);
        assert_eq!(content[0].content, vec![Cell::Header { content: vec![] }]);
    }

    #[test]
    fn wrong_node_type_is_refused() {
        assert!(serde_json::from_value::<Doc>(json!({"type": "paragraph"})).is_err());
        assert!(serde_json::from_value::<Doc>(
            json!({"type": "doc", "content": [{"type": "image"}]})
        )
        .is_err());
        let wrong_item = json!({"type": "doc", "content": [{"type": "bullet_list", "content": [{"type": "paragraph"}]}]});
        assert!(serde_json::from_value::<Doc>(wrong_item).is_err());
    }
}
