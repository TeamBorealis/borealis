//! A tiny XML DOM: just enough structure to represent the legacy element XML format.
//!
//! * DTDs and external entities are never processed (the XML reader ignores them).
//! * Whitespace-only text that contains a line break is formatting and is dropped, as with
//!   .NET's default `XmlDocument` loading. A whitespace-only text node on a single line
//!   (the space in `<i>a</i> <i>b</i>`) is content and is kept.
//! * `inner_xml` serializes like .NET `InnerXml`: `<br />` for empty elements, `&amp;`,
//!   `&lt;`, `&gt;` escaped in text.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use quick_xml::XmlVersion;

/// Deepest element nesting accepted.
///
/// This also makes the derived recursive `Drop`, `Clone`, `Debug` and `PartialEq` of the tree
/// safe: 128 levels is a few KiB of stack, so no iterative `Drop` is needed.
pub const MAX_DEPTH: usize = 128;
/// Most nodes (elements, text, CDATA, comments) accepted in one document.
pub const MAX_NODES: usize = 2_000_000;
/// Most attributes accepted on one element.
pub const MAX_ATTRS: usize = 256;

/// A parsed element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlElement {
    /// Tag name (with prefix, if any).
    pub name: String,
    /// Attributes in document order, values unescaped.
    pub attrs: Vec<(String, String)>,
    /// Child nodes in document order.
    pub children: Vec<XmlNode>,
    /// 1-based line of the start tag.
    pub line: u32,
}

/// A child node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlNode {
    /// An element.
    Element(XmlElement),
    /// Character data (unescaped).
    Text(String),
    /// A CDATA section.
    CData(String),
    /// A comment.
    Comment(String),
}

/// An XML syntax error with its line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct DomError {
    /// 1-based line.
    pub line: u32,
    /// What went wrong.
    pub message: String,
}

impl XmlElement {
    /// Attribute value by exact name.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Child elements in order.
    pub fn elements(&self) -> impl Iterator<Item = &XmlElement> {
        self.children.iter().filter_map(|n| match n {
            XmlNode::Element(e) => Some(e),
            _ => None,
        })
    }

    /// First child element with this tag name (exact).
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&XmlElement> {
        self.elements().find(|e| e.name == name)
    }

    /// True if any child is an element.
    #[must_use]
    pub fn has_element_children(&self) -> bool {
        self.elements().next().is_some()
    }

    /// Concatenated text and CDATA of this element and its descendants (like .NET
    /// `InnerText`: comments excluded).
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        collect_text(&self.children, &mut out);
        out
    }

    /// Serialize the children back to XML text (like .NET `InnerXml`).
    #[must_use]
    pub fn inner_xml(&self) -> String {
        let mut out = String::new();
        for child in &self.children {
            write_node(child, &mut out);
        }
        out
    }
}

fn collect_text(nodes: &[XmlNode], out: &mut String) {
    for n in nodes {
        match n {
            XmlNode::Text(t) | XmlNode::CData(t) => out.push_str(t),
            XmlNode::Element(e) => collect_text(&e.children, out),
            XmlNode::Comment(_) => {}
        }
    }
}

/// Escape character data.
pub(crate) fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
}

fn escape_attr(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
}

/// Serialize one node.
pub(crate) fn write_node(node: &XmlNode, out: &mut String) {
    match node {
        XmlNode::Text(t) => escape_text(t, out),
        XmlNode::CData(t) => {
            out.push_str("<![CDATA[");
            out.push_str(t);
            out.push_str("]]>");
        }
        XmlNode::Comment(t) => {
            out.push_str("<!--");
            out.push_str(t);
            out.push_str("-->");
        }
        XmlNode::Element(e) => {
            out.push('<');
            out.push_str(&e.name);
            for (k, v) in &e.attrs {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                escape_attr(v, out);
                out.push('"');
            }
            if e.children.is_empty() {
                out.push_str(" />");
            } else {
                out.push('>');
                for c in &e.children {
                    write_node(c, out);
                }
                out.push_str("</");
                out.push_str(&e.name);
                out.push('>');
            }
        }
    }
}

struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts }
    }

    fn line_of(&self, offset: usize) -> u32 {
        let idx = self.starts.partition_point(|&s| s <= offset);
        u32::try_from(idx).unwrap_or(u32::MAX)
    }
}

fn is_formatting_whitespace(text: &str) -> bool {
    text.chars().all(char::is_whitespace) && text.contains(['\n', '\r'])
}

fn start_element(start: &BytesStart<'_>, line: u32) -> Result<XmlElement, String> {
    let name = start.name().as_ref().to_string();
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        if attrs.len() >= MAX_ATTRS {
            return Err(format!("element has more than {MAX_ATTRS} attributes"));
        }
        let attr = attr.map_err(|e| e.to_string())?;
        let key = attr.key.as_ref().to_string();
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?
            .into_owned();
        attrs.push((key, value));
    }
    Ok(XmlElement {
        name,
        attrs,
        children: Vec::new(),
        line,
    })
}

fn flush_text(pending: &mut String, stack: &mut [XmlElement], root: &mut Option<XmlElement>) {
    if pending.is_empty() {
        return;
    }
    let value = std::mem::take(pending);
    if !stack.is_empty() && !is_formatting_whitespace(&value) {
        push_child(stack, root, XmlNode::Text(value));
    }
}

fn push_child(stack: &mut [XmlElement], root: &mut Option<XmlElement>, node: XmlNode) -> bool {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
        true
    } else if let XmlNode::Element(e) = node {
        if root.is_some() {
            return false;
        }
        *root = Some(e);
        true
    } else {
        true
    }
}

/// Parse a document and return its root element. A leading BOM is ignored.
///
/// # Errors
/// [`DomError`] for malformed XML, multiple roots or an empty document.
pub fn parse_document(text: &str) -> Result<XmlElement, DomError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines = Lines::new(text);
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;

    let mut stack: Vec<XmlElement> = Vec::new();
    let mut root: Option<XmlElement> = None;
    let mut nodes = 0usize;
    // Text arrives as Text / GeneralRef (`&amp;`) fragments; merge them into one node.
    let mut pending = String::new();

    loop {
        let offset = usize::try_from(reader.buffer_position()).unwrap_or(0);
        let line = lines.line_of(offset);
        let err = |message: String| DomError { line, message };
        let event = reader.read_event().map_err(|e| err(e.to_string()))?;
        if !matches!(
            event,
            Event::Eof | Event::Decl(_) | Event::PI(_) | Event::DocType(_)
        ) {
            nodes += 1;
            if nodes > MAX_NODES {
                return Err(err(format!("document has more than {MAX_NODES} nodes")));
            }
        }
        if !matches!(event, Event::Text(_) | Event::GeneralRef(_)) {
            flush_text(&mut pending, &mut stack, &mut root);
        }
        match event {
            Event::GeneralRef(r) => {
                if let Some(c) = r.resolve_char_ref().map_err(|e| err(e.to_string()))? {
                    pending.push(c);
                } else {
                    let name = r.into_inner();
                    let Some(rep) = quick_xml::escape::resolve_predefined_entity(&name) else {
                        return Err(err(format!("unrecognized entity `&{name};`")));
                    };
                    pending.push_str(rep);
                }
            }
            Event::Start(start) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(err(format!(
                        "elements nested deeper than {MAX_DEPTH} levels"
                    )));
                }
                stack.push(start_element(&start, line).map_err(err)?);
            }
            Event::Empty(start) => {
                let el = start_element(&start, line).map_err(err)?;
                if !push_child(&mut stack, &mut root, XmlNode::Element(el)) {
                    return Err(err("multiple root elements".into()));
                }
            }
            Event::End(_) => {
                let Some(done) = stack.pop() else {
                    return Err(err("unexpected closing tag".into()));
                };
                if !push_child(&mut stack, &mut root, XmlNode::Element(done)) {
                    return Err(err("multiple root elements".into()));
                }
            }
            Event::Text(t) => {
                pending.push_str(&t.xml10_content());
            }
            Event::CData(c) => {
                let value = c.into_inner().into_owned();
                push_child(&mut stack, &mut root, XmlNode::CData(value));
            }
            Event::Comment(c) => {
                let value = c.into_inner().into_owned();
                if !stack.is_empty() {
                    push_child(&mut stack, &mut root, XmlNode::Comment(value));
                }
            }
            Event::Eof => break,
            Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if let Some(open) = stack.last() {
        return Err(DomError {
            line: open.line,
            message: format!("unclosed element <{}>", open.name),
        });
    }
    root.ok_or(DomError {
        line: 1,
        message: "document has no root element".into(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn deep_nesting_is_rejected() {
        let text = "<a>".repeat(10_000);
        let e = super::parse_document(&text).unwrap_err();
        assert!(e.message.contains("nested deeper"), "{e}");
        let ok = format!("{}{}", "<a>".repeat(100), "</a>".repeat(100));
        assert!(super::parse_document(&ok).is_ok());
    }

    #[test]
    fn entities_merge_into_one_text_node() {
        let r = super::parse_document("<a>x &amp; y &#65; &lt;</a>").unwrap();
        assert_eq!(r.children.len(), 1);
        assert_eq!(r.children[0], super::XmlNode::Text("x & y A <".into()));
        assert!(super::parse_document("<a>&bogus;</a>").is_err());
    }

    #[test]
    fn too_many_attributes_are_rejected() {
        let attrs: String = (0..300).map(|i| format!(" k{i}=\"v\"")).collect();
        let e = super::parse_document(&format!("<a{attrs}/>")).unwrap_err();
        assert!(e.message.contains("attributes"), "{e}");
    }

    use super::*;

    #[test]
    fn parses_structure_attributes_and_lines() {
        let doc = parse_document(
            "<?xml version=\"1.0\"?>\n<a x=\"1 &amp; 2\">\n  <b/>\n  <c>t</c>\n</a>",
        )
        .unwrap();
        assert_eq!(doc.name, "a");
        assert_eq!(doc.attr("x"), Some("1 & 2"));
        assert_eq!(doc.elements().count(), 2);
        assert_eq!(doc.child("c").unwrap().line, 4);
        assert_eq!(doc.child("c").unwrap().text(), "t");
        assert!(doc.has_element_children());
    }

    #[test]
    fn bom_is_ignored_and_doctype_never_expanded() {
        let doc = parse_document("\u{feff}<!DOCTYPE a [<!ENTITY x \"boom\">]><a>ok</a>").unwrap();
        assert_eq!(doc.text(), "ok");
        assert!(parse_document("<a>&x;</a>").is_err());
    }

    #[test]
    fn inner_xml_matches_dotnet_style() {
        let doc = parse_document("<d>\n  <p class=\"x\">a &amp; b &lt; c<br/></p>\n  <p />\n</d>")
            .unwrap();
        assert_eq!(
            doc.inner_xml(),
            "<p class=\"x\">a &amp; b &lt; c<br /></p><p />"
        );
    }

    #[test]
    fn single_line_whitespace_between_inline_elements_is_kept() {
        let doc = parse_document("<d><i>a</i> <i>b</i>\n<i>c</i></d>").unwrap();
        assert_eq!(doc.inner_xml(), "<i>a</i> <i>b</i><i>c</i>");
    }

    #[test]
    fn cdata_comments_and_text() {
        let doc = parse_document("<d><![CDATA[<p>x</p>]]><!-- note --></d>").unwrap();
        assert_eq!(doc.text(), "<p>x</p>");
        assert_eq!(doc.inner_xml(), "<![CDATA[<p>x</p>]]><!-- note -->");
    }

    #[test]
    fn malformed_documents_report_lines() {
        for bad in ["<a><b></a>", "<a>", "", "<a/><b/>", "</a>", "<a x=1/>"] {
            assert!(parse_document(bad).is_err(), "{bad}");
        }
        let err = parse_document("<a>\n<b></c>\n</a>").unwrap_err();
        assert_eq!(err.line, 2);
    }
}
